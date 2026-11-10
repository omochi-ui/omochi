package fyi.omochi.mobile;

import java.io.File;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.nio.channels.FileLock;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

// Shared by every Flutter engine in the process, independent of Activity lifetime.
// The OS lock excludes other processes and is automatically released on a crash.
final class IdentityLeases {
  private static final Map<String, Lease> leases = new HashMap<>();

  private static final class Lease {
    final File path;
    final RandomAccessFile file;
    final FileLock lock;

    Lease(File path, RandomAccessFile file, FileLock lock) {
      this.path = path;
      this.file = file;
      this.lock = lock;
    }
  }

  static synchronized String acquire(File filesDirectory, String namespace) throws IOException {
    File directory = new File(filesDirectory, "identity-leases");
    if (!directory.mkdirs() && !directory.isDirectory()) {
      throw new IOException("Cannot create identity lease directory");
    }
    final byte[] digest;
    try {
      digest =
          MessageDigest.getInstance("SHA-256").digest(namespace.getBytes(StandardCharsets.UTF_8));
    } catch (NoSuchAlgorithmException error) {
      throw new IOException(error);
    }
    StringBuilder name = new StringBuilder();
    for (byte value : digest) name.append(String.format("%02x", value & 0xff));
    File path = new File(directory, name + ".lock").getCanonicalFile();
    // Check before opening: closing another channel for the same file can
    // release this JVM's existing OS lock on some platforms.
    for (Lease lease : leases.values()) {
      if (lease.path.equals(path)) throw new IOException("This Hub is already running");
    }
    RandomAccessFile file = new RandomAccessFile(path, "rw");
    try {
      FileLock lock = file.getChannel().tryLock();
      if (lock == null) throw new IOException("This Hub is already running");
      String token = UUID.randomUUID().toString();
      leases.put(token, new Lease(path, file, lock));
      return token;
    } catch (IOException | RuntimeException error) {
      try {
        file.close();
      } catch (IOException closeError) {
        error.addSuppressed(closeError);
      }
      throw error;
    }
  }

  static synchronized void release(String token) throws IOException {
    Lease lease = leases.get(token);
    if (lease == null) return;
    // Retain the token on failure so Dart keeps its guard and can retry.
    lease.lock.release();
    lease.file.close();
    leases.remove(token);
    // Never unlink: another process may already be locking this inode.
  }
}

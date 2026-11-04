// Builds the same embedded Hub used on mobile; no external Hub is required.
import 'dart:io';

Future<void> main(List<String> arguments) async {
  try {
    final options = <String, String>{};
    for (var i = 0; i < arguments.length; i += 2) {
      if (i + 1 >= arguments.length || !arguments[i].startsWith('--')) {
        throw const FormatException('Expected --name value arguments');
      }
      options[arguments[i].substring(2)] = arguments[i + 1];
    }
    final target = switch (options['target']) {
      'linux-x64' => 'x86_64-unknown-linux-gnu',
      'linux-arm64' => 'aarch64-unknown-linux-gnu',
      'windows-x64' => 'x86_64-pc-windows-msvc',
      'windows-arm64' => 'aarch64-pc-windows-msvc',
      _ => throw FormatException(
        'Unsupported desktop target: ${options['target']}',
      ),
    };
    final windows = target.contains('windows');
    if (windows ? !Platform.isWindows : !Platform.isLinux) {
      throw const FormatException('Build on the target desktop OS');
    }
    final release = switch (options['mode']) {
      'Debug' => false,
      'Profile' || 'Release' => true,
      _ => throw FormatException('Unsupported build mode: ${options['mode']}'),
    };
    final output = Directory(
      options['output'] ?? (throw const FormatException('Missing --output')),
    ).absolute;
    final app = File.fromUri(Platform.script).parent.parent;
    final root = Directory(
      Platform.environment['OMOCHI_HUB_ROOT'] ?? app.parent.parent.path,
    ).absolute;
    final manifest = File.fromUri(root.uri.resolve('crates/Cargo.toml'));
    if (!manifest.existsSync()) {
      throw const FormatException('Omochi crates/Cargo.toml is missing');
    }
    final targetDir = Directory(
      Platform.environment['CARGO_TARGET_DIR'] ?? '${root.path}/crates/target',
    ).absolute;
    final home = Platform.environment[windows ? 'USERPROFILE' : 'HOME'];
    final userCargo = home == null
        ? null
        : File('$home/.cargo/bin/cargo${windows ? '.exe' : ''}');
    final cargo =
        Platform.environment['CARGO'] ??
        (userCargo != null && userCargo.existsSync()
            ? userCargo.path
            : 'cargo');
    final process = await Process.start(
      cargo,
      [
        'rustc',
        '--lib',
        '--crate-type',
        'cdylib',
        '--locked',
        '--manifest-path',
        manifest.path,
        '-p',
        'omochi-native-bridge',
        '--target',
        target,
        if (release) '--release',
      ],
      environment: {'CARGO_TARGET_DIR': targetDir.path},
      mode: ProcessStartMode.inheritStdio,
    );
    final status = await process.exitCode;
    if (status != 0) {
      exitCode = status;
      return;
    }
    final library = windows
        ? 'omochi_native_bridge.dll'
        : 'libomochi_native_bridge.so';
    final source = File(
      '${targetDir.path}/$target/${release ? 'release' : 'debug'}/$library',
    );
    output.createSync(recursive: true);
    final staging = Directory.fromUri(output.uri.resolve('.hub-staging/'));
    staging.createSync(recursive: true);
    final staged = await source.copy('${staging.path}/$library');
    final destination = File('${output.path}/$library');
    // A mapped Windows DLL cannot be replaced. Fail the build in that case;
    // never leave a successful bundle with an old Hub. POSIX rename is atomic.
    if (windows && destination.existsSync()) await destination.delete();
    await staged.rename(destination.path);
    await staging.delete();
  } catch (error) {
    stderr.writeln('Embedded Hub build failed: $error');
    exitCode = 1;
  }
}

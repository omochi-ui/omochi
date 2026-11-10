use omochi_process::{Command, SIGKILL, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[test]
#[ignore = "subprocess fixture"]
fn child_fixture() {
    match std::env::var("OMOCHI_PROCESS_FIXTURE").as_deref() {
        Ok("echo") => {
            let argument = std::env::var("OMOCHI_PROCESS_ARGUMENT").unwrap();
            println!("argument:{argument}");
            println!("argv:{}", std::env::args().next_back().unwrap());
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
            println!("input:{text}");
        }
        Ok("tree") => {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "child_fixture", "--nocapture"])
                .env("OMOCHI_PROCESS_FIXTURE", "descendant")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let _ = child.wait();
        }
        Ok("descendant") => {
            // Keep the inherited writer open for this process's entire lifetime.
            let mut stdout = std::io::stdout().lock();
            std::io::Write::write_all(&mut stdout, b"grandchild ready\n").unwrap();
            std::io::Write::flush(&mut stdout).unwrap();
            loop {
                std::thread::park();
            }
        }
        Ok("wait") => loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        },
        #[cfg(windows)]
        Ok("graceful") => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let mut shutdown = omochi_process::ShutdownSignal::new().unwrap();
                println!("ready for shutdown");
                shutdown.wait().await;
            });
        }
        _ => panic!("fixture mode missing"),
    }
}
#[tokio::test]
async fn pipes_environment_and_exit_status() {
    let text = "spaces 日本語 \\\" quotes \\";
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "child_fixture", "--nocapture"])
        .args(["--skip", text])
        .env("OMOCHI_PROCESS_FIXTURE", "echo")
        .env("OMOCHI_PROCESS_ARGUMENT", text)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"hello")
        .await
        .unwrap();
    drop(child.stdin.take());
    let mut out = String::new();
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_to_string(&mut out)
        .await
        .unwrap();
    assert!(child.wait().await.unwrap().success(), "{out}");
    assert!(out.contains(&format!("argument:{text}")), "{out}");
    assert!(out.contains(&format!("argv:{text}")), "{out}");
    assert!(out.contains("input:hello"), "{out}");
}

#[cfg(windows)]
#[tokio::test]
async fn console_break_allows_graceful_child_shutdown() {
    use windows_sys::Win32::System::Console::{AllocConsole, GetConsoleCP};
    if unsafe { GetConsoleCP() } == 0 {
        assert_ne!(unsafe { AllocConsole() }, 0);
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "child_fixture", "--nocapture"])
        .env("OMOCHI_PROCESS_FIXTURE", "graceful")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if line == "ready for shutdown" {
                return;
            }
        }
        panic!("child exited before registering shutdown");
    })
    .await
    .unwrap();
    child.group().signal(omochi_process::SIGTERM);
    let status = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success(), "{status}");
}
#[tokio::test]
async fn cancellation_terminates_an_idle_child() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "child_fixture"])
        .env("OMOCHI_PROCESS_FIXTURE", "wait")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let group = child.group();
    group.signal(SIGKILL);
    let status = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(!status.success());
}

#[tokio::test]
async fn cancellation_stops_grandchildren() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "child_fixture", "--nocapture"])
        .env("OMOCHI_PROCESS_FIXTURE", "tree")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut line = String::new();
        loop {
            assert_ne!(
                stdout.read_line(&mut line).await.unwrap(),
                0,
                "grandchild exited before readiness"
            );
            // libtest may prefix the marker with its own progress output.
            if line.contains("grandchild ready") {
                break;
            }
            line.clear();
        }
    })
    .await
    .expect("grandchild did not become ready");
    child.group().signal(SIGKILL);
    let status = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
        .await
        .expect("child did not terminate")
        .unwrap();
    assert!(!status.success());
    // EOF requires both the parent and descendant to close their inherited writers.
    // Retain the Child guard so a failed assertion still tears down its process tree.
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        stdout.read_to_end(&mut Vec::new()),
    )
    .await
    .expect("grandchild retained its stdout pipe after cancellation")
    .unwrap();
}

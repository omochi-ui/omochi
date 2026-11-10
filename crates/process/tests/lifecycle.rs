use omochi_process::{Command, SIGKILL, Stdio};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
                .env("OMOCHI_PROCESS_FIXTURE", "heartbeat")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let _ = child.wait();
        }
        Ok("heartbeat") => {
            let path = std::env::var_os("OMOCHI_PROCESS_HEARTBEAT").unwrap();
            for sequence in 0u64..=u64::MAX {
                std::fs::write(&path, sequence.to_string()).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(10));
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
    use tokio::io::{AsyncBufReadExt, BufReader};
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
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("heartbeat");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "child_fixture", "--nocapture"])
        .env("OMOCHI_PROCESS_FIXTURE", "tree")
        .env("OMOCHI_PROCESS_HEARTBEAT", &path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !path.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    child.group().signal(SIGKILL);
    child.wait().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let before = std::fs::read(&path).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "grandchild is still running"
    );
}

use std::{path::Path, time::Duration, process::Stdio};

use tokio::{
    process::Command,
    sync::oneshot::{self, Sender},
};

pub fn start_firefox_driver(
    driver_path: impl AsRef<Path>,
    browser_path: impl AsRef<Path>,
) -> Sender<()> {
    let driver_path = driver_path.as_ref().as_os_str().to_owned();
    let browser_path = browser_path.as_ref().to_str().unwrap().to_owned();
    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let mut child = Command::new(driver_path)
            .args(["-b", &browser_path])
            .stdout(Stdio::null())
            .spawn()
            .expect("driver start failed");
        let _ = rx.await;
        child.kill().await.expect("drive kill error");
        tracing::debug!("driver killed");
    });
    //waiting for driver started
    std::thread::sleep(Duration::from_secs(1));
    tracing::debug!("driver started");
    tx
}

use anyhow::anyhow;
use tokio::{fs::{read_link, remove_file, symlink}, io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, process::Command};

pub async fn is_healthy(port: u16) -> bool {
    async fn is_healthy_inner(port: u16) -> anyhow::Result<()> {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).await?;
        stream.write_all(b"GET / HTTP/1.1\r\n\r\n").await?;
        let mut buf = [0; 1];
        let bytes = stream.read(&mut buf).await?;
        if bytes == 0 {
            return Err(anyhow!("No response"));
        }

        Ok(())
    } 

    is_healthy_inner(port).await.is_ok()
}

pub async fn checking_symlink(original: &str, link: &str) -> anyhow::Result<bool> {
    let previous_link = read_link(link).await?;
    let expected_link = &original;

    if previous_link.to_str() == Some(expected_link) {
        return Ok(false);
    }

    // Replace nginx config with hibernator config
    remove_file(link).await.map_err(|e| anyhow!("could not remove previous symlink: {e}"))?;
    symlink(original, link).await.map_err(|e| anyhow!("could not create symlink: {e}"))?;
    Ok(true)
}

pub async fn run_command(command: &str) -> anyhow::Result<()> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .await
        .map_err(|e| anyhow!("could not run command: {e}"))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("command failed: {command} {stdout} {stderr}"));
    }

    Ok(())
}

pub fn now() -> u64 {
    chrono::Utc::now().timestamp() as u64
}

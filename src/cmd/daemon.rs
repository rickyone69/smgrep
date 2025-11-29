//! Daemon connection and lifecycle management.
//!
//! Handles connecting to existing daemon processes, spawning new ones when
//! needed, and performing version handshakes to ensure compatibility.

use std::{
   path::Path,
   process::{Command, Stdio},
   time::Duration,
};

use tokio::time;

use crate::{
   Result,
   error::Error,
   ipc::{Request, Response, SocketBuffer},
   usock, version,
};

/// Maximum number of connection retry attempts when waiting for daemon startup.
const RETRY_COUNT: usize = 50;
/// Delay between retry attempts.
const RETRY_DELAY: Duration = Duration::from_millis(100);

/// Connects to a daemon instance matching the current version, spawning one if
/// needed.
///
/// First attempts to connect to an existing daemon. If successful and versions
/// match, returns the connection. Otherwise spawns a new daemon and waits for
/// it to be ready.
pub async fn connect_matching_daemon(path: &Path, store_id: &str) -> Result<usock::Stream> {
   if let Some(stream) = try_connect_existing(store_id).await? {
      return Ok(stream);
   }

   spawn_daemon(path)?;
   wait_for_daemon(store_id).await
}

/// Spawns a new daemon process in the background for the given path.
pub fn spawn_daemon(path: &Path) -> Result<()> {
   let exe = std::env::current_exe()?;

   Command::new(&exe)
      .arg("serve")
      .arg("--path")
      .arg(path)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()?;

   Ok(())
}

/// Waits for a newly spawned daemon to become available and respond to
/// handshakes.
async fn wait_for_daemon(store_id: &str) -> Result<usock::Stream> {
   for _ in 0..RETRY_COUNT {
      time::sleep(RETRY_DELAY).await;
      if let Some(stream) = try_connect_existing(store_id).await? {
         return Ok(stream);
      }
   }

   Err(Error::Server {
      op:     "handshake",
      reason: "daemon did not start with matching version".to_string(),
   })
}

/// Attempts to connect to an existing daemon and verify version compatibility
/// via handshake.
async fn try_connect_existing(store_id: &str) -> Result<Option<usock::Stream>> {
   match usock::Stream::connect(store_id).await {
      Ok(mut stream) => {
         if matches!(handshake(&mut stream).await, Ok(true)) {
            Ok(Some(stream))
         } else {
            force_shutdown(Some(stream), store_id).await?;
            Ok(None)
         }
      },
      Err(_) => Ok(None),
   }
}

/// Performs a version handshake with a daemon to ensure compatibility.
async fn handshake(stream: &mut usock::Stream) -> Result<bool> {
   let mut buffer = SocketBuffer::new();
   let request = Request::Hello { git_hash: version::GIT_HASH.to_string() };
   buffer.send(stream, &request).await?;

   match buffer.recv::<_, Response>(stream).await {
      Ok(Response::Hello { git_hash }) => Ok(git_hash == version::GIT_HASH),
      Ok(_) => Err(Error::UnexpectedResponse("handshake")),
      Err(e) => Err(e),
   }
}

/// Forces a daemon to shut down and removes its socket.
pub async fn force_shutdown(existing: Option<usock::Stream>, store_id: &str) -> Result<()> {
   let mut buffer = SocketBuffer::new();

   if let Some(mut stream) = existing {
      let _ = buffer.send(&mut stream, &Request::Shutdown).await;
      let _ = buffer.recv::<_, Response>(&mut stream).await;
   } else if let Ok(mut stream) = usock::Stream::connect(store_id).await {
      let _ = buffer.send(&mut stream, &Request::Shutdown).await;
      let _ = buffer.recv::<_, Response>(&mut stream).await;
   }

   usock::remove_socket(store_id);
   Ok(())
}

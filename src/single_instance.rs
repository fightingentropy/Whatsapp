//! Single-instance coordination over a loopback socket.
//!
//! A second launch asks the existing process to show its window and exits.
//! The operating system releases the loopback port when the process ends.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Fixed high loopback port outside the ephemeral range.
const INSTANCE_PORT: u16 = 47_120;

/// Independent wire identity: a fork launch must never control upstream.
const PREFIX: &str = "whatsapp:";
const OK_REPLY: &str = "whatsapp:ok";
// Share the guard with our previous app while a user is upgrading. Never
// contact or accept the upstream client's protocol or port.
const PREVIOUS_PREFIX: &str = "zapfast-silicon:";

pub enum Outcome {
    /// This process owns the instance guard.
    Only(Guard),
    /// The existing instance was asked to show its window.
    Surfaced,
}

/// Request from another launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlCommand {
    /// Shows or creates the window.
    Show,
}

/// Owns the listener that marks this process as the running instance.
pub struct Guard {
    /// Requests queued by later launches.
    commands: Arc<Mutex<Vec<ControlCommand>>>,
}

impl Guard {
    /// Shared request queue drained by the app.
    pub fn commands(&self) -> Arc<Mutex<Vec<ControlCommand>>> {
        Arc::clone(&self.commands)
    }
}

/// Sends one request and verifies the Whatsapp reply prefix.
pub fn send(verb: &str) -> std::io::Result<()> {
    send_to(INSTANCE_PORT, verb)
}

fn send_to(port: u16, verb: &str) -> std::io::Result<()> {
    send_with_prefix(port, verb, PREFIX).or_else(|_| send_with_prefix(port, verb, PREVIOUS_PREFIX))
}

fn send_with_prefix(port: u16, verb: &str, prefix: &str) -> std::io::Result<()> {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(format!("{prefix}{verb}\n").as_bytes())?;
    // Read the one-line reply until the connection closes.
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    if reply.lines().next() == Some(format!("{prefix}ok").as_str()) {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the port is held by something other than Whatsapp",
        ))
    }
}

pub fn acquire(waker: &crate::backend::Waker) -> Outcome {
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, INSTANCE_PORT)) {
        Ok(listener) => listener,
        Err(_) => {
            // If the port is held, continue only when it is not Whatsapp.
            if send("show").is_ok() {
                return Outcome::Surfaced;
            }
            log::warn!("port {INSTANCE_PORT} is busy but not with Whatsapp; running unguarded");
            return Outcome::Only(Guard {
                commands: Default::default(),
            });
        }
    };
    let guard = Guard {
        commands: Default::default(),
    };
    let commands = Arc::clone(&guard.commands);
    let waker = waker.clone();
    let spawned = std::thread::Builder::new()
        .name("whatsapp-instance".to_owned())
        .spawn(move || serve(listener, &commands, &waker));
    if let Err(error) = spawned {
        log::warn!("cannot listen for other launches: {error}");
    }
    Outcome::Only(guard)
}

/// Handles one request and reply per connection until the listener closes.
fn serve(
    listener: TcpListener,
    commands: &Mutex<Vec<ControlCommand>>,
    waker: &crate::backend::Waker,
) {
    for mut stream in listener.incoming().flatten() {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let Some(line) = read_line(&mut stream) else {
            continue;
        };
        // Ignore clients without the Whatsapp prefix.
        if let Some(command) = parse(&line) {
            let reply = if line.starts_with(PREVIOUS_PREFIX) {
                "zapfast-silicon:ok"
            } else {
                OK_REPLY
            };
            let _ = stream.write_all(format!("{reply}\n").as_bytes());
            commands
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(command);
            waker.wake();
        }
    }
}

fn parse(line: &str) -> Option<ControlCommand> {
    let line = line.trim_end();
    match line
        .strip_prefix(PREFIX)
        .or_else(|| line.strip_prefix(PREVIOUS_PREFIX))?
    {
        "show" => Some(ControlCommand::Show),
        _ => None,
    }
}

/// Reads a bounded line and rejects read errors or oversized input.
fn read_line(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = [0u8; 256];
    let mut filled = 0;
    loop {
        if filled == buffer.len() {
            return None;
        }
        match stream.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => {
                filled += read;
                if buffer[..filled].contains(&b'\n') {
                    break;
                }
            }
            Err(_) => return None,
        }
    }
    let line = buffer[..filled].split(|&byte| byte == b'\n').next()?;
    String::from_utf8(line.to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_own_show_is_understood() {
        assert_eq!(parse("whatsapp:show\n"), Some(ControlCommand::Show));
        assert_eq!(parse("whatsapp:show"), Some(ControlCommand::Show));
        assert_eq!(parse("GET / HTTP/1.1"), None);
        assert_eq!(parse("whatsapp:frobnicate"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("zapfast-silicon:show"), Some(ControlCommand::Show));
        assert_eq!(parse("zapfast:show"), None);
        assert_eq!(parse("fastsapp:show"), None);
    }

    /// Verifies a request crosses the socket into the app queue.
    #[test]
    fn a_second_launch_reaches_the_queue() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        let commands: Arc<Mutex<Vec<ControlCommand>>> = Default::default();
        let served = {
            let commands = Arc::clone(&commands);
            let waker = crate::backend::Waker::default();
            std::thread::spawn(move || serve(listener, &commands, &waker))
        };

        send_to(port, "show").expect("answered as Whatsapp");
        send_with_prefix(port, "show", PREVIOUS_PREFIX)
            .expect("the previous app can surface Whatsapp during an upgrade");
        // Unknown verbs close the connection without a reply.
        assert!(send_to(port, "frobnicate").is_err());

        assert_eq!(
            *commands.lock().expect("the queue"),
            vec![ControlCommand::Show, ControlCommand::Show]
        );
        drop(served);
    }

    #[test]
    fn an_upgrade_surfaces_the_previous_app_before_moving_its_data() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = std::thread::spawn(move || {
            // The previous release rejects the new prefix, then accepts the
            // fallback. It must retain sole ownership of its open databases.
            let (mut first, _) = listener.accept().unwrap();
            assert_eq!(read_line(&mut first).as_deref(), Some("whatsapp:show"));
            drop(first);
            let (mut second, _) = listener.accept().unwrap();
            assert_eq!(
                read_line(&mut second).as_deref(),
                Some("zapfast-silicon:show")
            );
            second.write_all(b"zapfast-silicon:ok\n").unwrap();
        });
        send_to(port, "show").unwrap();
        served.join().unwrap();
    }
}

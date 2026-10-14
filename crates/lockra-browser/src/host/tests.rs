use std::sync::{Arc, Mutex};

use tokio::io::DuplexStream;

use super::*;
use crate::frame::MAX_FRAME;

/// The machine as a test sets it up: an app to connect to (its ends handed to the test), or none.
#[derive(Clone, Default)]
struct Machine {
    app: Arc<Mutex<Option<mpsc::Sender<DuplexStream>>>>,
    started: Arc<Mutex<Vec<bool>>>,
}

impl Machine {
    /// An app is running: each connection's app end comes out of the receiver.
    fn run_app(&self) -> mpsc::Receiver<DuplexStream> {
        let (sender, receiver) = mpsc::channel(4);
        *self.app.lock().unwrap() = Some(sender);
        receiver
    }

    fn stop_app(&self) {
        *self.app.lock().unwrap() = None;
    }
}

impl HostPorts for Machine {
    type Stream = DuplexStream;

    async fn connect(&self) -> io::Result<DuplexStream> {
        let sender = self.app.lock().unwrap().clone();
        let Some(sender) = sender else { return Err(io::Error::new(io::ErrorKind::ConnectionRefused, "not running")) };
        let (host, app) = tokio::io::duplex(2 * MAX_FRAME);
        sender.send(app).await.map_err(|_| io::Error::new(io::ErrorKind::ConnectionRefused, "gone"))?;
        Ok(host)
    }

    fn start_app(&self, show: bool) -> io::Result<()> {
        self.started.lock().unwrap().push(show);
        Ok(())
    }
}

fn hello() -> Vec<u8> {
    Envelope::new(Kind::Hello, br#"{"op":"hello","proto_min":1,"proto_max":1,"browser":"chrome","extension":"e","version":"0.10.0"}"#).frame()
}

fn start(show: bool) -> Vec<u8> {
    let plain = if show { br#"{"op":"start_app","show":true}"#.to_vec() } else { br#"{"op":"start_app"}"#.to_vec() };
    Envelope::new(Kind::Hello, &plain).frame()
}

/// The browser's side of a host: what it writes in, and what the host writes back.
struct Browser {
    input: DuplexStream,
    output: DuplexStream,
    host: JoinHandle<Result<(), FrameError>>,
}

fn host(machine: &Machine) -> Browser {
    let (input, host_input) = tokio::io::duplex(2 * MAX_FRAME);
    let (host_output, output) = tokio::io::duplex(2 * MAX_FRAME);
    let host = tokio::spawn(relay(host_input, host_output, machine.clone()));
    Browser { input, output, host }
}

impl Browser {
    async fn send(&mut self, frame: &[u8]) {
        write_frame(&mut self.input, Order::Native, frame).await.unwrap();
    }

    async fn next(&mut self) -> Vec<u8> {
        tokio::time::timeout(std::time::Duration::from_secs(5), read_frame(&mut self.output, Order::Native)).await.unwrap().unwrap().unwrap()
    }
}

#[tokio::test]
async fn without_the_app_every_frame_is_told_lockra_is_not_running() {
    let machine = Machine::default();
    let mut browser = host(&machine);
    browser.send(&hello()).await;
    assert_eq!(browser.next().await, not_running());
    let plain: AppPlain = serde_json::from_slice(&serde_json::from_slice::<Envelope>(&not_running()).unwrap().bytes().unwrap()).unwrap();
    assert_eq!(plain, AppPlain::State { state: AppState::NotRunning });
    assert_eq!(
        String::from_utf8(serde_json::from_slice::<Envelope>(&not_running()).unwrap().bytes().unwrap()).unwrap(),
        r#"{"op":"state","state":"not_running"}"#
    );
    // A session's frame too: nothing is passed on, nothing waits.
    browser.send(&Envelope::new(Kind::Msg, b"sealed").frame()).await;
    assert_eq!(browser.next().await, not_running());
    drop(browser.input);
    assert!(browser.host.await.unwrap().is_ok());
}

#[tokio::test]
async fn start_app_starts_lockra_and_is_not_answered() {
    let machine = Machine::default();
    let mut browser = host(&machine);
    browser.send(&start(false)).await;
    browser.send(&start(true)).await;
    // The hello after it is the first frame answered.
    browser.send(&hello()).await;
    assert_eq!(browser.next().await, not_running());
    assert_eq!(*machine.started.lock().unwrap(), vec![false, true]);
}

#[tokio::test]
async fn frames_pass_both_ways_in_each_legs_byte_order() {
    let machine = Machine::default();
    let mut apps = machine.run_app();
    let mut browser = host(&machine);
    browser.send(&hello()).await;
    let mut app = apps.recv().await.unwrap();
    // The app reads it little-endian, as the socket's frames are.
    assert_eq!(read_frame(&mut app, Order::Little).await.unwrap().unwrap(), hello());
    let answer = Envelope::new(Kind::Hello, br#"{"op":"hello","proto_min":1,"proto_max":1,"proto":1,"app_version":"0.10.0","state":"locked"}"#).frame();
    write_frame(&mut app, Order::Little, &answer).await.unwrap();
    assert_eq!(browser.next().await, answer);
    // The same connection carries the next frames; start_app still stays with the host.
    browser.send(&start(true)).await;
    let sealed = Envelope::new(Kind::Msg, b"sealed").frame();
    browser.send(&sealed).await;
    assert_eq!(read_frame(&mut app, Order::Little).await.unwrap().unwrap(), sealed);
    assert_eq!(*machine.started.lock().unwrap(), vec![true]);
}

#[tokio::test]
async fn as_the_app_goes_the_browser_hears_it_and_the_next_frame_connects_again() {
    let machine = Machine::default();
    let mut apps = machine.run_app();
    let mut browser = host(&machine);
    browser.send(&hello()).await;
    let app = apps.recv().await.unwrap();
    drop(app);
    assert_eq!(browser.next().await, not_running());
    // Lockra quit: the next hello is told so.
    machine.stop_app();
    browser.send(&hello()).await;
    assert_eq!(browser.next().await, not_running());
    // It runs again: the next hello reaches it.
    let mut apps = machine.run_app();
    browser.send(&hello()).await;
    let mut app = apps.recv().await.unwrap();
    assert_eq!(read_frame(&mut app, Order::Little).await.unwrap().unwrap(), hello());
}

#[tokio::test]
async fn what_is_not_a_frame_ends_the_host() {
    let machine = Machine::default();
    let mut browser = host(&machine);
    // A length over the limit, in the browser's byte order.
    let length = u32::try_from(MAX_FRAME + 1).unwrap().to_ne_bytes();
    tokio::io::AsyncWriteExt::write_all(&mut browser.input, &length).await.unwrap();
    assert!(matches!(browser.host.await.unwrap(), Err(FrameError::TooLong(_))));
}

#[test]
fn only_a_plain_start_app_is_one() {
    assert_eq!(start_request(&start(false)), Some(false));
    assert_eq!(start_request(&start(true)), Some(true));
    assert_eq!(start_request(&hello()), None);
    assert_eq!(start_request(&Envelope::new(Kind::Msg, br#"{"op":"start_app"}"#).frame()), None);
    assert_eq!(start_request(b"not json"), None);
}

use std::{env, io::Write, os::unix::net::UnixStream, process, thread, time::Duration};

use revision_supervisor::CandidateEvent;

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "stay".into());
    if mode == "crash-before" {
        process::exit(41);
    }
    if mode == "no-ready" {
        loop {
            thread::sleep(Duration::from_secs(60));
        }
    }
    let event = CandidateEvent::FirstFrame {
        token: env::var("SOS_SUPERVISOR_TOKEN").expect("supervisor token"),
        revision_id: env::var("SOS_REVISION_ID").expect("revision id"),
    };
    let mut stream =
        UnixStream::connect(env::var("SOS_SUPERVISOR_SOCKET").expect("supervisor socket"))
            .expect("connect readiness socket");
    serde_json::to_writer(&mut stream, &event).expect("serialize event");
    stream.write_all(b"\n").expect("write event");
    stream.flush().expect("flush event");
    drop(stream);

    match mode.as_str() {
        "crash-after" => {
            thread::sleep(Duration::from_millis(40));
            process::exit(42);
        }
        "exit-after" => {
            thread::sleep(Duration::from_millis(40));
        }
        _ => loop {
            thread::sleep(Duration::from_secs(60));
        },
    }
}

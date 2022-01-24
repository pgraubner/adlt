/// integration tests for full binary
use assert_cmd::Command;
use portpicker::pick_unused_port;
use predicates::prelude::*;
use std::time::{Duration, Instant};

#[test]
fn bin_version() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    let assert = cmd.arg("-V").assert();
    assert
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")))
        .success();
}

#[test]
fn bin_convert_notext() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    let assert = cmd.args(&["convert", "foo.dlt"]).assert();
    assert.failure();
}

#[test]
fn bin_remote_invalidport() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    let assert = cmd.args(&["remote", "-v", "-p", "1"]).assert();
    println!("{:?}", assert.get_output());
    assert.failure();
}

#[test]
fn bin_remote_validport_listen() {
    let port: u16 = pick_unused_port().expect("no ports free");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    let assert = cmd
        .args(&["remote", "-v", "-p", &format!("{}", port)])
        .timeout(std::time::Duration::from_secs(1))
        .assert()
        .stderr(predicate::str::contains(format!(
            "remote server listening on 127.0.0.1:{}",
            port
        )))
        .interrupted();
    println!("{:?}", assert.get_output());
}

#[test]
fn bin_remote_validport_connect() {
    let port: u16 = pick_unused_port().expect("no ports free");
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    // start the client that connects and sends close
    let t = std::thread::spawn(move || {
        println!("trying to connect to webclient at port {}", port);

        let mut ws;
        let start_time = Instant::now();
        loop {
            match tungstenite::client::connect(format!("wss://127.0.0.1:{}", port)) {
                Ok(p) => {
                    ws = p.0;
                    break;
                }
                Err(_e) => {
                    if start_time.elapsed() > Duration::from_secs(1) {
                        panic!("couldnt connect");
                    } else {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            }
        }
        println!(
            "connected to webclient at port {} after {}ms",
            port,
            start_time.elapsed().as_millis()
        );

        ws.write_message(tungstenite::protocol::Message::Text("close".to_string()))
            .unwrap();
        let answer = ws.read_message().unwrap();
        assert!(answer.is_text());
        assert_eq!(
            answer.into_text().unwrap(),
            "err: close failed as no file open. open first!"
        );
        std::thread::sleep(Duration::from_millis(20));
    });

    let assert = cmd
        .args(&["remote", "-v", "-p", &format!("{}", port)])
        .timeout(std::time::Duration::from_secs(1))
        .assert()
        .stderr(predicate::str::contains(format!(
            "remote server listening on 127.0.0.1:{}",
            port
        )))
        .stderr(predicate::str::contains("got text message \"close\""))
        .interrupted();
    println!("{:?}", assert.get_output());
    t.join().unwrap();
}
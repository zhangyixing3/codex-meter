#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unsafe_op_in_unsafe_fn)]

mod data;
mod ui;
use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|s| s == "--check") {
        let mut s = data::Snapshot::default();
        if let Err(e) = data::refresh(&mut s) {
            s.error = Some(e);
        }
        // GUI-subsystem binaries write to redirected stdout, e.g. PowerShell pipelines.
        println!("{}", serde_json::to_string_pretty(&s).unwrap());
        std::process::exit(if s.error.is_some() { 1 } else { 0 });
    }
    let demo = args.iter().any(|s| s == "--demo");
    let shared = Arc::new(Mutex::new(if demo {
        data::Snapshot::demo()
    } else {
        data::Snapshot::load()
    }));
    if let Some(i) = args.iter().position(|s| s == "--preview") {
        let path = args
            .get(i + 1)
            .expect("--preview requires a BMP output path");
        unsafe {
            ui::preview(&shared.lock().unwrap(), path).expect("preview failed");
        }
        return;
    }
    if let Some(i) = args.iter().position(|s| s == "--preview-reset") {
        let path = args
            .get(i + 1)
            .expect("--preview-reset requires a BMP path");
        unsafe {
            ui::preview_reset(&shared.lock().unwrap(), path).expect("preview failed");
        }
        return;
    }
    let Some(_instance) = (unsafe { ui::single_instance() }) else {
        return;
    };
    let (tx, rx) = mpsc::channel();
    let worker = if !demo {
        let state = shared.clone();
        Some(std::thread::spawn(move || {
            loop {
                let mut next = {
                    let mut s = state.lock().unwrap();
                    s.refreshing = true;
                    s.notice = None;
                    s.clone()
                };
                next.error = data::refresh(&mut next).err();
                next.refreshing = false;
                next.notice = Some(
                    if next.error.is_some() {
                        "刷新失败"
                    } else if next.usage_error.is_some() {
                        "额度已刷新 · Token 更新失败"
                    } else {
                        "额度和 Token 已刷新"
                    }
                    .into(),
                );
                next.save();
                *state.lock().unwrap() = next;
                match rx.recv_timeout(Duration::from_secs(300)) {
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    _ => while rx.try_recv().is_ok() {},
                }
            }
        }))
    } else {
        None
    };
    unsafe {
        ui::run(shared, tx, args.iter().any(|s| s == "--hidden"));
    }
    // Allow the in-flight read to reap its child before this process exits.
    if let Some(worker) = worker {
        let _ = worker.join();
    }
}

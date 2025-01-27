use app_global::AppGlobal;
use futures::{executor::block_on, AsyncReadExt, AsyncWriteExt, StreamExt};
use gpui::*;
use smol::net::unix::{UnixListener, UnixStream};
use std::{ffi::OsString, io, os::unix::ffi::{OsStrExt, OsStringExt}, path::PathBuf, process::exit};

pub mod line_edit;
pub mod dialog;
pub mod models;
pub mod views;
pub mod app_global;

async fn handle_client(cx: &mut AsyncAppContext, stream: &mut UnixStream) -> io::Result<()> {
    let mut szbuf = [0u8; 2];
    stream.read_exact(&mut szbuf).await?;
    let sz = u16::from_le_bytes(szbuf);
    let mut data = vec![0;sz as usize];
    let _ = stream.read_exact(data.as_mut_slice()).await?;
    let target = PathBuf::from(OsString::from_vec(data));
    AppGlobal::new_main_window(target, cx);

    Ok(())
}

fn main() {
    let target = PathBuf::from(std::env::args().nth(1).unwrap_or(std::env::var("HOME").unwrap()));
    if !target.exists() || !target.metadata().is_ok_and(|m| m.is_dir()) {
        eprintln!("{} is not a dir", target.display());
        exit(-1);
    }

    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".to_string());
    let sock_path = runtime_dir + "/forg.sock";
    // Try to connect to domain socket and send what we want to open.

    let opened = block_on(async {
        let Ok(mut stream) = UnixStream::connect(sock_path.clone()).await else {
            return false;
        };
        let szbuf = (target.capacity() as u16).to_le_bytes();
        let _ = stream.write_all(&szbuf).await;
        let _ = stream.write_all(target.as_os_str().as_bytes()).await;
        return true;
    });

    if opened {
        exit(-1);
    }

    App::new().run(|cx: &mut AppContext| {
        let mut async_cx = cx.to_async();
        cx.foreground_executor().spawn(async move {
            if std::fs::exists(&sock_path).unwrap_or(false) {
                let _ = std::fs::remove_file(&sock_path);
            }
            let listener = UnixListener::bind(sock_path).expect("Cannot listen");
            let mut incoming = listener.incoming();
            while let Some(s) = incoming.next().await {
                let Ok(mut stream) = s else {
                    eprintln!("Cannot accept client socket");
                    continue;
                };
                let _ = handle_client(&mut async_cx, &mut stream).await;
                let _ = stream.close().await;
            }
        }).detach();

        println!("Scanning icons and mime databases");
        cx.set_global(AppGlobal::new());
        println!("Done");

        cx.spawn(|mut cx| async move {
            AppGlobal::new_main_window(target, &mut cx);
        }).detach();
    });
}

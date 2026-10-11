//! Print the text of every QR code in an image, one per line (the desktop smoke test reads the
//! export screen's screenshot with it). Usage: `cargo run -p lockra-transfer --example decode-qr -- shot.png`

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: decode-qr <image>");
        return ExitCode::from(2);
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("decode-qr: {path}: {error}");
            return ExitCode::from(2);
        }
    };
    match lockra_transfer::qr::decode_image(&bytes) {
        Ok(found) if found.is_empty() => {
            eprintln!("decode-qr: no QR code in {path}");
            ExitCode::FAILURE
        }
        Ok(found) => {
            for text in found {
                println!("{}", text.as_str());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("decode-qr: {path}: {error}");
            ExitCode::from(2)
        }
    }
}

//! Print the SVG of a QR code holding the given text (the documentation site's screenshot tool
//! draws a Google Authenticator export code with it).
//! Usage: `cargo run -p lockra-transfer --example encode-qr -- 'otpauth-migration://offline?data=…'`

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(text) = std::env::args().nth(1) else {
        eprintln!("usage: encode-qr <text>");
        return ExitCode::from(2);
    };
    match lockra_transfer::qr::svg(&text) {
        Ok(svg) => {
            print!("{}", svg.as_str());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("encode-qr: {error}");
            ExitCode::FAILURE
        }
    }
}

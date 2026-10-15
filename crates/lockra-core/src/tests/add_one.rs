//! One account added at once from a QR code or a link (the clipboard's image or text, an image
//! file), for a login's form to link it (board 07-c, 「新建一个」): exactly one account; several go
//! through the import on the Codes page.

use super::*;

fn clipboard_qr(h: &Harness, data: &str) {
    let png = png_of_qr(data);
    let rgba = image::load_from_memory(&png).unwrap().to_rgba8();
    h.clipboard.put_image(ClipboardImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() });
}

fn google(accounts: &[(&str, &str)]) -> String {
    let accounts: Vec<lockra_otp::OtpAuth> = accounts.iter().map(|(issuer, secret)| uri::parse(&otpauth(issuer, "me", secret)).unwrap()).collect();
    let refs: Vec<&lockra_otp::OtpAuth> = accounts.iter().collect();
    let codes = lockra_transfer::google::encode(&refs).unwrap();
    assert_eq!(codes.len(), 1);
    codes[0].uri.as_str().to_owned()
}

#[tokio::test(start_paused = true)]
async fn the_clipboard_adds_its_one_account_at_once() {
    let h = Harness::unlocked().await;
    assert_eq!(code_err(h.core.add_from_clipboard().await), ErrorCode::ClipboardEmpty);
    clipboard_qr(&h, &otpauth("Shot", "x", "GEZDGNBV"));
    let id = h.core.add_from_clipboard().await.unwrap();
    let state = h.core.state();
    let entry = state.entries.iter().find(|e| e.id == id).unwrap();
    assert_eq!((entry.issuer.as_str(), entry.account.as_str(), entry.origin), ("Shot", "x", lockra_transfer::Origin::Uri));
    assert!(state.import.is_none(), "not through the import");
    // A link copied as text, the same.
    h.clipboard.put_text(&otpauth("Text", "y", "MZXW6YTBOI"));
    h.core.add_from_clipboard().await.unwrap();
    // Again: that account is in the vault already.
    assert_eq!(code_err(h.core.add_from_clipboard().await), ErrorCode::DuplicateEntry);
    assert_eq!(h.core.state().entries.len(), 2);
    h.clipboard.fail.store(true, Ordering::SeqCst);
    assert_eq!(code_err(h.core.add_from_clipboard().await), ErrorCode::ClipboardFailed);
}

#[tokio::test(start_paused = true)]
async fn several_accounts_go_through_the_import() {
    let h = Harness::unlocked().await;
    h.clipboard.put_text(&format!("{}\n{}", otpauth("A", "a", "GEZDGNBV"), otpauth("B", "b", "MZXW6YTBOI")));
    assert_eq!(code_err(h.core.add_from_clipboard().await), ErrorCode::SeveralAccounts);
    // A Google Authenticator export code with two accounts.
    clipboard_qr(&h, &google(&[("G1", "GEZDGNBV"), ("G2", "MZXW6YTBOI")]));
    assert_eq!(code_err(h.core.add_from_clipboard().await), ErrorCode::SeveralAccounts);
    assert!(h.core.state().entries.is_empty(), "nothing added");
    // One with a single account adds it, as from Google.
    clipboard_qr(&h, &google(&[("G3", "MFRGGZDF")]));
    let id = h.core.add_from_clipboard().await.unwrap();
    let state = h.core.state();
    let entry = state.entries.iter().find(|e| e.id == id).unwrap();
    assert_eq!((entry.issuer.as_str(), entry.origin), ("G3", lockra_transfer::Origin::Google));
}

#[tokio::test(start_paused = true)]
async fn an_image_file_adds_its_one_account() {
    let h = Harness::unlocked().await;
    let dir = tempfile::tempdir().unwrap();
    let shot = dir.path().join("qr.png");
    fs::write(&shot, png_of_qr(&otpauth("File", "z", "GEZDGNBV"))).unwrap();
    let id = h.core.add_from_image(shot).await.unwrap();
    assert!(h.core.state().entries.iter().any(|e| e.id == id && e.issuer == "File"));
    // An image without a code, a file that is no image, one that is not there.
    let blank = dir.path().join("blank.png");
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_luma8(64, 64).write_to(&mut out, image::ImageFormat::Png).unwrap();
    fs::write(&blank, out.into_inner()).unwrap();
    assert_eq!(code_err(h.core.add_from_image(blank).await), ErrorCode::ImportEmpty);
    let note = dir.path().join("note.png");
    fs::write(&note, b"not an image").unwrap();
    assert_eq!(code_err(h.core.add_from_image(note).await), ErrorCode::ImportUnreadable);
    assert_eq!(code_err(h.core.add_from_image(dir.path().join("gone.png")).await), ErrorCode::ImportUnreadable);
    assert_eq!(h.core.state().entries.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn nothing_is_added_from_a_link_it_cannot_take_or_while_locked() {
    let h = Harness::unlocked().await;
    h.clipboard.put_text("otpauth://totp/A:b?secret=GEZDGNBV&algorithm=MD5");
    assert_eq!(code_err(h.core.add_from_clipboard().await), ErrorCode::ImportEmpty);
    h.core.lock_vault();
    h.clipboard.put_text(&otpauth("A", "a", "GEZDGNBV"));
    assert_eq!(code_err(h.core.add_from_clipboard().await), ErrorCode::Locked);
    let dir = tempfile::tempdir().unwrap();
    let shot = dir.path().join("qr.png");
    fs::write(&shot, png_of_qr(&otpauth("File", "z", "GEZDGNBV"))).unwrap();
    assert_eq!(code_err(h.core.add_from_image(shot).await), ErrorCode::Locked);
}

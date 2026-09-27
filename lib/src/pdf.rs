//! Printable two-page wallet: receive and view on page one, recovery on page two.

use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use printpdf::*;
use qrcode::{types::Color as QrColor, QrCode};

use crate::paper::{Pool, WalletRecord};

const WIDTH: f32 = 210.0;
const HEIGHT: f32 = 297.0;

pub fn save_to_pdf(records: &[WalletRecord], filename: &Path) -> Result<(), String> {
    if records.is_empty() {
        return Err("Cannot print an empty paper wallet".to_owned());
    }
    let mut doc = PdfDocument::new("Pirate Chain Paper Wallet");
    let regular = BuiltinFont::Helvetica;
    let bold = BuiltinFont::HelveticaBold;
    let mono = BuiltinFont::Courier;
    let total_pages = records.len() * 2;
    let mut pages = Vec::with_capacity(total_pages);

    for (index, wallet) in records.iter().enumerate() {
        let mut receiving = Vec::new();
        draw_receiving(
            &mut doc,
            &mut receiving,
            &regular,
            &bold,
            &mono,
            wallet,
            index * 2 + 1,
            total_pages,
        )?;
        pages.push(PdfPage::new(Mm(WIDTH), Mm(HEIGHT), receiving));

        let mut recovery = Vec::new();
        draw_recovery(
            &mut doc,
            &mut recovery,
            &regular,
            &bold,
            &mono,
            wallet,
            index * 2 + 2,
            total_pages,
        )?;
        pages.push(PdfPage::new(Mm(WIDTH), Mm(HEIGHT), recovery));
    }

    // QR codes must remain lossless and must not be resized by image optimization.
    let options = PdfSaveOptions {
        image_optimization: None,
        ..PdfSaveOptions::default()
    };
    let bytes = doc.with_pages(pages).save(&options, &mut Vec::new());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(filename)
        .map_err(|e| format!("{}: {e}", filename.display()))?;
    if let Err(error) = file.write_all(&bytes) {
        drop(file);
        let cleanup = std::fs::remove_file(filename);
        return Err(match cleanup {
            Ok(()) => format!("{}: {error}", filename.display()),
            Err(cleanup_error) => format!(
                "{}: {error}; could not remove incomplete PDF: {cleanup_error}",
                filename.display()
            ),
        });
    }
    Ok(())
}

fn text(
    layer: &mut Vec<Op>,
    value: impl Into<String>,
    size: f32,
    x: Mm,
    y: Mm,
    font: &BuiltinFont,
) {
    layer.extend([
        Op::StartTextSection,
        Op::SetTextCursor {
            pos: Point::new(x, y),
        },
        Op::SetFont {
            font: PdfFontHandle::Builtin(*font),
            size: Pt(size),
        },
        Op::ShowText {
            items: vec![TextItem::Text(value.into())],
        },
        Op::EndTextSection,
    ]);
}

fn draw_header(
    layer: &mut Vec<Op>,
    regular: &BuiltinFont,
    bold: &BuiltinFont,
    title: &str,
    subtitle: &str,
    page: usize,
    total: usize,
) {
    text(layer, title, 22.0, Mm(18.0), Mm(278.0), bold);
    text(layer, subtitle, 10.0, Mm(18.0), Mm(266.0), regular);
    text(
        layer,
        format!("Page {page} of {total}"),
        9.0,
        Mm(173.0),
        Mm(12.0),
        regular,
    );
}

fn draw_receiving(
    doc: &mut PdfDocument,
    layer: &mut Vec<Op>,
    regular: &BuiltinFont,
    bold: &BuiltinFont,
    mono: &BuiltinFont,
    wallet: &WalletRecord,
    page: usize,
    total: usize,
) -> Result<(), String> {
    draw_header(
        layer,
        regular,
        bold,
        "Pirate Chain Paper Wallet",
        &format!(
            "{} account {} | {}",
            wallet.pool.name(),
            wallet.num,
            wallet.seed.path
        ),
        page,
        total,
    );
    if wallet.pool == Pool::Ironwood {
        text(
            layer,
            "Use Ironwood after network activation. Confirm the chain is active before funding.",
            8.5,
            Mm(18.0),
            Mm(257.0),
            regular,
        );
    }
    text(layer, "Receiving address", 14.0, Mm(18.0), Mm(248.0), bold);
    add_qr(doc, layer, &wallet.address, 18.0, 185.0, 55.0)?;
    draw_chunks(layer, mono, &wallet.address, 44, 10.0, 80.0, 228.0, 7.0);

    text(
        layer,
        "Additional receive addresses",
        13.0,
        Mm(18.0),
        Mm(172.0),
        bold,
    );
    for (i, address) in [
        &wallet.diversified.d1,
        &wallet.diversified.d2,
        &wallet.diversified.d3,
        &wallet.diversified.d4,
        &wallet.diversified.d5,
    ]
    .iter()
    .enumerate()
    {
        let y = 160.0 - (i as f32 * 14.0);
        text(layer, format!("{}.", i + 1), 9.0, Mm(18.0), Mm(y), bold);
        text(layer, address.as_str(), 9.0, Mm(29.0), Mm(y), mono);
    }

    text(
        layer,
        "Extended viewing key",
        13.0,
        Mm(18.0),
        Mm(88.0),
        bold,
    );
    add_qr(doc, layer, &wallet.viewing_key, 18.0, 24.0, 55.0)?;
    draw_chunks(layer, mono, &wallet.viewing_key, 52, 8.5, 80.0, 76.0, 6.7);
    text(
        layer,
        "The viewing key can reveal wallet activity. Share it only when intended.",
        8.5,
        Mm(18.0),
        Mm(13.0),
        regular,
    );
    Ok(())
}

fn draw_recovery(
    doc: &mut PdfDocument,
    layer: &mut Vec<Op>,
    regular: &BuiltinFont,
    bold: &BuiltinFont,
    mono: &BuiltinFont,
    wallet: &WalletRecord,
    page: usize,
    total: usize,
) -> Result<(), String> {
    draw_header(
        layer,
        regular,
        bold,
        "Spending and Recovery",
        &format!(
            "{} account {} | Keep this page private and offline",
            wallet.pool.name(),
            wallet.num
        ),
        page,
        total,
    );
    text(
        layer,
        "Extended spending key",
        14.0,
        Mm(18.0),
        Mm(246.0),
        bold,
    );
    add_qr(doc, layer, &wallet.private_key, 18.0, 178.0, 58.0)?;
    draw_chunks(layer, mono, &wallet.private_key, 45, 9.0, 85.0, 229.0, 7.0);

    text(layer, "Recovery phrase", 13.0, Mm(18.0), Mm(164.0), bold);
    let words: Vec<&str> = wallet.seed.phrase.split_whitespace().collect();
    if words.len() == 24 {
        for (line, group) in words.chunks(6).enumerate() {
            text(
                layer,
                format!(
                    "{:02}-{:02}  {}",
                    line * 6 + 1,
                    line * 6 + 6,
                    group.join(" ")
                ),
                10.0,
                Mm(20.0),
                Mm(151.0 - line as f32 * 12.0),
                mono,
            );
        }
    } else {
        text(layer, &wallet.seed.phrase, 10.0, Mm(20.0), Mm(151.0), mono);
    }

    text(layer, "HD seed (hex)", 11.0, Mm(18.0), Mm(94.0), bold);
    text(layer, &wallet.seed.hdseed, 9.5, Mm(20.0), Mm(83.0), mono);
    text(layer, "BIP39 seed (hex)", 11.0, Mm(18.0), Mm(68.0), bold);
    draw_chunks(
        layer,
        mono,
        &wallet.seed.bip39_seed,
        64,
        9.0,
        20.0,
        57.0,
        8.0,
    );
    text(
        layer,
        format!("Derivation path: {}", wallet.seed.path),
        10.0,
        Mm(18.0),
        Mm(30.0),
        regular,
    );
    text(
        layer,
        "Import the spending key to access funds. Store this page securely.",
        8.5,
        Mm(18.0),
        Mm(13.0),
        regular,
    );
    Ok(())
}

fn draw_chunks(
    layer: &mut Vec<Op>,
    font: &BuiltinFont,
    value: &str,
    width: usize,
    size: f32,
    x: f32,
    y: f32,
    line_spacing: f32,
) {
    for (line, chunk) in value.as_bytes().chunks(width).enumerate() {
        // Keys and addresses are ASCII Bech32 or hex.
        let chunk_text = std::str::from_utf8(chunk).expect("Encoded wallet value is ASCII");
        text(
            layer,
            chunk_text,
            size,
            Mm(x),
            Mm(y - line as f32 * line_spacing),
            font,
        );
    }
}

fn add_qr(
    doc: &mut PdfDocument,
    layer: &mut Vec<Op>,
    data: &str,
    x: f32,
    y: f32,
    width_mm: f32,
) -> Result<(), String> {
    let code = QrCode::new(data.as_bytes()).map_err(|e| format!("QR generation failed: {e}"))?;
    let modules = code.width();
    let colors = code.to_colors();
    let scale = 4;
    let quiet = 4;
    let side = (modules + quiet * 2) * scale;
    let mut pixels = Vec::with_capacity(side * side * 3);
    for row in 0..side {
        for col in 0..side {
            let module_row = row / scale;
            let module_col = col / scale;
            let dark = module_row >= quiet
                && module_col >= quiet
                && module_row < modules + quiet
                && module_col < modules + quiet
                && colors[(module_row - quiet) * modules + (module_col - quiet)] == QrColor::Dark;
            pixels.extend_from_slice(if dark { &[0, 0, 0] } else { &[255, 255, 255] });
        }
    }
    let image = RawImage {
        width: side,
        height: side,
        pixels: RawImageData::U8(pixels),
        data_format: RawImageFormat::RGB8,
        tag: Vec::new(),
    };
    let id = doc.add_image(&image);
    let dpi = side as f32 * 25.4 / width_mm;
    layer.push(Op::UseXobject {
        id,
        transform: XObjectTransform {
            translate_x: Some(Mm(x).into()),
            translate_y: Some(Mm(y).into()),
            dpi: Some(dpi),
            ..XObjectTransform::default()
        },
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper::{generate_wallet, SeedSource, WalletOptions};

    #[test]
    fn writes_ironwood_pdf_with_long_keys() {
        let seed = [7; 32];
        let wallets = generate_wallet(SeedSource::HdSeed(&seed), WalletOptions::default()).unwrap();
        let path =
            std::env::temp_dir().join(format!("pirate-paper-wallet-{}.pdf", std::process::id()));
        save_to_pdf(&wallets, &path).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(original.len() > 1000);
        assert!(save_to_pdf(&wallets, &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_file(path).unwrap();
    }
}

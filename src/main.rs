extern crate bstr;
use bstr::ByteSlice;
use flate2::read::GzDecoder;
use mimalloc::MiMalloc;
use parking_lot::Mutex;
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::prelude::*;
use std::io::{self, SeekFrom};
use std::sync::Arc;
use std::env;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> io::Result<()> {
    //let homes = fs::read_dir("/home/piotr/mail/")?.collect::<Result<Vec<_>, io::Error>>()?;
    let homes =
        fs::read_dir("/root/docker/dovecot/data/mail/")?.collect::<Result<Vec<_>, io::Error>>()?;
    let dirs = vec![
        "Maildir/new/",
        "Maildir/cur/",
        "Maildir/.Trash/new/",
        "Maildir/.Trash/cur/",
        "Maildir/.Junk/new/",
        "Maildir/.Junk/cur/",
    ];
    let mut maildirs = vec![];
    for mbox in homes {
        for dir in &dirs {
            let mut path = mbox.path();
            path.push(&dir);
            maildirs.push(path);
        }
    }
    let files_arc = Arc::new(Mutex::new(Vec::with_capacity(50_000))); // vector of tuples (direntry, modification date)
                                                                      // we saving time here to do less statx calls during sorting
    maildirs.par_iter().for_each(|mdir| {
        if let Ok(mut dir_iter) = fs::read_dir(&mdir) {
            let clone = Arc::clone(&files_arc);
            while let Some(Ok(file)) = dir_iter.next() {
                if let Ok(date) = file.metadata().and_then(|md| md.modified()) {
                    let mut files_vec = clone.lock();
                    files_vec.push((file, date));
                }
            }
        }
    });
    //dbg!(a);
    let mut files = files_arc.lock();
    dbg!(&files.len());
    files.par_sort_unstable_by(|a, b| b.1.cmp(&a.1));

    //for item in files.iter().take(5) {
    //    dbg!(item);
    //}

    let needle = match env::args().nth(1) {
        Some(n) => n,
        None => return Ok(()),
    };

    let _found = files.par_iter().take(300).find_any(|file| {
        let filename = file.0.path();
        let mut buffer = vec![0; 4 * 1024];
        let mut two_bytes = vec![0u8; 2];
        let gz_magic: &[u8] = &[0x1F, 0x8B];
        let mut f = File::open(&filename).unwrap();
        f.read_exact(&mut two_bytes).unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        let gzipped = two_bytes == gz_magic;
        if gzipped {
            let mut d = GzDecoder::new(&f);
            d.read(&mut buffer).unwrap();
        } else {
            f.read(&mut buffer).unwrap();
        }
        if buffer.contains_str(&needle[..]) {
            println!("foundz: {} in {}", needle, &filename.display());
            let mut buf = vec![];
            f.seek(SeekFrom::Start(0)).unwrap();
            if gzipped {
                let mut d = GzDecoder::new(&f);
                d.read_to_end(&mut buf).unwrap();
            } else {
                f.read_to_end(&mut buf).unwrap();
            }
            let parsed = parse_mail(&buf).unwrap();
            display_email(parsed);
            println!("{}", &filename.display());
            true
        } else {
            false
        }
    });

    //dbg!(found);
    Ok(())
}

extern crate august;
extern crate htmlescape;
extern crate mailparse;
extern crate term_size;
use mailparse::*;

fn display_email(parsed: ParsedMail) {
    let subject = parsed.headers.get_first_value("Subject").unwrap();
    if let Some(s) = subject {
        println!("Subject: {}", s);
    }
    println!("mimetype: {}", parsed.ctype.mimetype);
    if parsed.ctype.mimetype == "text/plain".to_string() {
        let body = parsed.get_body().unwrap();
        let body = htmlescape::decode_html(&body).unwrap_or(body);
        println!("{}", body);
    } else if parsed.ctype.mimetype == "text/html".to_string() {
        let body = parsed.get_body().unwrap();
        let mut width = 79;
        if let Some((w, _h)) = term_size::dimensions() {
            width = w;
        }
        println!("{}", august::convert(&body, width));
    }
    //println!("Subparts: {}", parsed.subparts.len());
    for part in parsed.subparts {
        display_email(part);
    }
}

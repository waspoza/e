extern crate bstr;
use bstr::ByteSlice;
use flate2::read::GzDecoder;
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::prelude::*;
use std::io::{self, SeekFrom};
use std::sync::Arc;
use std::{cmp, env};
use parking_lot::Mutex;
use mimalloc::MiMalloc;
use tokio::task::JoinSet;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> io::Result<()> {
    //let homes = fs::read_dir("/home/piotr/mail/")?.collect::<Result<Vec<_>, io::Error>>()?;
    let homes = fs::read_dir("/root/docker/dovecot/data/mail/")?.collect::<Result<Vec<_>, io::Error>>()?;
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
    let mut files = Vec::with_capacity(50_000); // vector of tuples (direntry, modification date)
                                            // we saving time here to do less statx calls during sorting
    let mut task_set = JoinSet::new();
    for mdir in maildirs {
        task_set.spawn(async move {
            let mut dir_content = vec![];
            if let Ok(mut dir_iter) = tokio::fs::read_dir(&mdir).await {
                while let Ok(Some(file)) = dir_iter.next_entry().await {
                    //file = file.unwrap();
                    let date = file.metadata().await.unwrap().modified().unwrap();
                    dir_content.push((file, date));
                }
            }
            dir_content
        });
    }
    while let Some(Ok(mut content)) = task_set.join_next().await {
        files.append(&mut content);
    }
    //dbg!(a);
    dbg!(&files.len());
    files.par_sort_unstable_by(|a, b| b.1.cmp(&a.1));

    //for item in files.iter().take(5) {
    //    dbg!(item);
    //}

    let needle = match env::args().nth(1) {
        Some(n) => n,
        None => return Ok(()),
    };

    let min = cmp::min(files.len(), 300);
    for file in files.iter().take(min) {
        let filename = file.0.path();
        let mut buffer = vec![0; 4 * 1024];
        let mut two_bytes = vec![0u8; 2];
        let gz_magic: &[u8] = &[0x1F, 0x8B];
        let mut f = File::open(&filename)?;
        f.read_exact(&mut two_bytes)?;
        f.seek(SeekFrom::Start(0))?;
        let gzipped = two_bytes == gz_magic;
        if gzipped {
            let mut d = GzDecoder::new(&f);
            d.read(&mut buffer)?;
        } else {
            f.read(&mut buffer)?;
        }
        if buffer.contains_str(&needle[..]) {
            println!("foundz: {} in {}", needle, &filename.display());
            let mut buf = vec![];
            f.seek(SeekFrom::Start(0))?;
            if gzipped {
                let mut d = GzDecoder::new(&f);
                d.read_to_end(&mut buf)?;
            } else {
                f.read_to_end(&mut buf)?;
            }
            let parsed = parse_mail(&buf).unwrap();
            display_email(parsed);
            println!("{}", &filename.display());
            break;
        }
    }
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

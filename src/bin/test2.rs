use std::fs;
use std::io;
fn main() -> Result<(), std::io::Error> {
    let homes = fs::read_dir("/home/piotr/mail/")?.collect::<Result<Vec<_>, io::Error>>()?;
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

    let mut files = vec![];
    for mdir in maildirs {
        for file in fs::read_dir(&mdir)? {
            files.push(file?);
        }
    }
    dbg!(&files.len());
    files.sort_unstable_by(|a, b| {
        b.metadata()
            .expect("meta")
            .modified()
            .expect("mod")
            .cmp(&a.metadata().expect("meta").modified().expect("mod"))
    });

    for item in files.iter().take(5) {
        dbg!(item);
        dbg!(item.metadata()?.modified()?);
    }
    Ok(())
}

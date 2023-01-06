extern crate bstr;
use bstr::ByteSlice;
use flate2::read::GzDecoder;
use nc::syscalls::{syscall2, syscall3};
use std::cmp;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, SeekFrom};

fn main() -> io::Result<()> {
    let mut homes = Dirlist::new(BufSize::Small);
    //homes.read_directory("/home/piotr/mail/", false).unwrap();
    homes
        .read_directory("/root/docker/dovecot/data/mail/", Get::OnlyDirs)
        .unwrap();

    let dirs = vec![
        "/Maildir/new/",
        "/Maildir/cur/",
        "/Maildir/.Trash/new/",
        "/Maildir/.Trash/cur/",
        "/Maildir/.Junk/new/",
        "/Maildir/.Junk/cur/",
    ];
    let mut maildirs = vec![];
    for mbox in homes.get_filenames() {
        for dir in &dirs {
            maildirs.push(mbox.clone() + dir);
        }
    }
    //dbg!(&maildirs);
    let mut files = Dirlist::new(BufSize::Big);
    for mdir in maildirs {
        files.read_directory(&mdir, Get::OnlyFiles).unwrap();
    }
    dbg!(&files.len());
    files.sort();
    //files.check_mtime();
    //for idx in 0..5 {
    //  dbg!(files.get_filename(idx));
    //}

    let needle = match env::args().nth(1) {
        Some(n) => n,
        None => return Ok(()),
    };

    let min = cmp::min(files.len(), 300);
    for idx in 0..min {
        let file = files.get_filename(idx).unwrap();
        let mut buffer = vec![0; 4 * 1024];
        let mut two_bytes = vec![0u8; 2];
        let gz_magic: &[u8] = &[0x1F, 0x8B];
        let mut f = File::open(&file)?;
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
            println!("foundz: {} in {}", needle, file);
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
            println!("{}", file);
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

extern crate alloc;
extern crate nc;
use std::ffi::CStr;

enum BufSize {
    Big,
    Small,
}

enum Get {
    OnlyFiles,
    OnlyDirs,
}

struct Dirlist {
    buf_box_ptr: usize, // item buffer
    buf_size: usize,
    free_space_ptr: usize,
    items: Vec<(usize, usize)>, // item addreses with index to directory names
    dirnames: Vec<String>,
}

impl Dirlist {
    fn new(buf: BufSize) -> Dirlist {
        let buf_size: usize = if let BufSize::Big = buf {
            5_048_576
        } else {
            4096
        };
        let buf: Vec<u8> = vec![0; buf_size];
        let buf_box = buf.into_boxed_slice();
        let buf_box_ptr = alloc::boxed::Box::into_raw(buf_box) as *mut u8 as usize;
        let free_space_ptr = buf_box_ptr;
        let items: Vec<(usize, usize)> = Vec::new();
        let dirnames: Vec<String> = Vec::new();
        Dirlist {
            buf_box_ptr,
            buf_size,
            free_space_ptr,
            items,
            dirnames,
        }
    }
    fn read_directory(&mut self, dirname: &str, what_to_get: Get) -> Result<(), nc::Errno> {
        if dirname.chars().last().unwrap() != '/' {
            panic!("dirname must end with a slash!");
        }
        let dir_index = match self.dirnames.iter().position(|x| x == dirname) {
            Some(index) => index,
            None => {
                self.dirnames.push(dirname.to_string());
                self.dirnames.len() - 1
            }
        };
        let fd = match nc::openat(nc::AT_FDCWD, dirname, nc::O_RDONLY | nc::O_DIRECTORY, 0) {
            Ok(fd) => fd,
            Err(_) => return Ok(()),
        };
        if let Get::OnlyFiles = what_to_get {
            nc::chdir(dirname)?;
        }
        loop {
            unsafe {
                let free_space_left = self.buf_size - (self.free_space_ptr - self.buf_box_ptr);
                if free_space_left <= 0 {
                    panic!("No free space left in buffer sized: {}", self.buf_size);
                }
                let nread = syscall3(
                    nc::SYS_GETDENTS64,
                    fd as usize,
                    self.free_space_ptr,
                    free_space_left,
                )?;
                if nread == 0 {
                    break;
                }
                let mut bpos = 0;
                while bpos < nread {
                    let item_addr = self.free_space_ptr + bpos;
                    let d = item_addr as *mut nc::linux_dirent64_t;
                    bpos = bpos + (*d).d_reclen as usize;
                    if let Get::OnlyFiles = what_to_get {
                        if (*d).d_type as nc::mode_t == nc::DT_DIR {
                            continue;
                        } else {
                            let mut statbuf = nc::stat_t::default();
                            let statbuf_ptr = &mut statbuf as *mut nc::stat_t as usize;
                            let name_ptr = &mut (*d).d_name[0] as *mut u8 as usize;
                            syscall2(nc::SYS_LSTAT, name_ptr, statbuf_ptr).map(|_ret| ())?;
                            (*d).d_ino = statbuf.st_mtime as u64; // replace inode number with file modification time
                        }
                    } else {
                        if (*d).d_type as nc::mode_t == nc::DT_REG {
                            continue;
                        }
                        if (*d).d_name[0] == '.' as u8 && (*d).d_name[1] == 0 {
                            continue;
                        }
                        if (*d).d_name[0] == '.' as u8
                            && (*d).d_name[1] == '.' as u8
                            && (*d).d_name[2] == 0
                        {
                            continue;
                        }
                    }
                    self.items.push((item_addr, dir_index));
                }
                self.free_space_ptr += nread;
            }
        }
        let _ = nc::close(fd).expect("Failed to close fd");
        Ok(())
    }
    fn len(&self) -> usize {
        self.items.len()
    }
    fn get_filenames(&self) -> Vec<String> {
        let mut result = vec![];
        for idx in 0..self.items.len() {
            result.push(self.get_filename(idx).unwrap());
        }
        result
    }
    fn get_filename(&self, index: usize) -> Option<String> {
        if index > self.items.len() {
            return None;
        }
        let (item_addr, dir_index) = self.items[index];
        let mut fullname = self.dirnames[dir_index].clone(); // get the dir part
        let item = item_addr as *mut nc::linux_dirent64_t;
        let d_name: &CStr = unsafe { CStr::from_ptr((*item).d_name.as_ptr().cast()) };
        let filename: &str = d_name.to_str().unwrap();
        fullname.push_str(filename); // and add the file part
        Some(fullname)
    }
    fn sort(&mut self) {
        self.items.sort_by(|a, b| {
            let item1 = a.0 as *mut nc::linux_dirent64_t;
            let item2 = b.0 as *mut nc::linux_dirent64_t;
            unsafe { (*item1).d_ino.cmp(&(*item2).d_ino).reverse() }
        });
    }
    #[allow(dead_code)]
    fn check_mtime(&self) {
        for (item_addr, _dir_index) in (&self.items).into_iter().take(5) {
            let item = *item_addr as *mut nc::linux_dirent64_t;
            unsafe {
                dbg!((*item).d_ino);
            }
        }
    }
}

struct DirlistIntoIterator {
    dirlist: Dirlist,
    index: usize,
}

impl IntoIterator for Dirlist {
    type Item = String;
    type IntoIter = DirlistIntoIterator;

    fn into_iter(self) -> Self::IntoIter {
        DirlistIntoIterator {
            dirlist: self,
            index: 0,
        }
    }
}

impl Iterator for DirlistIntoIterator {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        if self.index >= self.dirlist.items.len() {
            return None;
        }
        let (item_addr, dir_index) = self.dirlist.items[self.index];
        let mut filename = self.dirlist.dirnames[dir_index].to_owned();
        let item = item_addr as *mut nc::linux_dirent64_t;
        for i in 0..nc::PATH_MAX {
            unsafe {
                let c = (*item).d_name[i as usize];
                if c == 0 {
                    break;
                }
                filename.push(c as char);
            }
        }
        self.index += 1;
        Some(filename)
    }
}
/*
fn dents(dirname: &str, only_files: bool, entries: &mut Vec<Entry>)  {
    if dirname.chars().last().unwrap() != '/' {
        panic!("dirname must end with slash!");
    }

    let fd = match nc::openat(nc::AT_FDCWD, dirname, nc::O_RDONLY | nc::O_DIRECTORY, 0) {
        Ok(fd) => fd,
        Err(_) => return
    };

    loop {
        match mygetdents(fd, dirname, only_files, entries) {
            Ok(size) => {
                if size == 0 {
                    break;
                }
            }
            Err(err) => {
                eprintln!("err: {}", err);
                break;
            }
        }
    }

    let _ = nc::close(fd).expect("Failed to close fd");
}


fn mygetdents(fd: i32, dirname: &str, only_files: bool, entries: &mut Vec<Entry>) -> Result<usize, nc::Errno> {
    const BUF_SIZE: usize = 1_048_576;
    let mut now = SystemTime::now();
    unsafe {
        let buf: Vec<u8> = vec![0; BUF_SIZE];
        let buf_box = buf.into_boxed_slice();
        let buf_box_ptr = alloc::boxed::Box::into_raw(buf_box) as *mut u8 as usize;
        let fd = fd as usize;
        let nread = nc::syscall3(nc::SYS_GETDENTS64, fd, buf_box_ptr, BUF_SIZE)?;

        if nread == 0 {
            return Ok(nread);
        }

        let mut bpos = 0;
        while bpos < nread {
            let d = (buf_box_ptr + bpos) as *mut nc::linux_dirent64_t;
            bpos = bpos + (*d).d_reclen as usize;
            let mut name_vec: Vec<u8> = dirname.as_bytes().to_owned();
            if only_files {
                if (*d).d_type as nc::mode_t == nc::DT_DIR {
                    continue;
                }
            } else {
                if (*d).d_type as nc::mode_t == nc::DT_REG {
                    continue;
                }
                if (*d).d_name[0] == '.' as u8 && (*d).d_name[1] == 0 {
                    continue;
                }
                if (*d).d_name[0] == '.' as u8 && (*d).d_name[1] == '.' as u8 && (*d).d_name[2] == 0 {
                    continue;
                }
            }
            for i in 0..nc::PATH_MAX {
                let c = (*d).d_name[i as usize];
                if c == 0 {
                    break;
                }
                name_vec.push(c);
            }
            if !only_files && (*d).d_type as nc::mode_t == nc::DT_DIR {
                name_vec.push('/' as u8);
            }
            let name = String::from_utf8(name_vec).unwrap();
            if only_files {
                //let mut statbuf = nc::stat_t::default();
                //nc::newfstatat(nc::AT_FDCWD, &name[..], &mut statbuf, 0)?;
                //dbg!(&statbuf);
                let meta = metadata(&name).unwrap();
                now = meta.modified().unwrap();
            }

            entries.push(Entry {
                time: now,
                path: name,
            });
        }
        return Ok(nread);
    }
}
*/

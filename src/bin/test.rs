#![allow(unused)]
fn main() {
    use std::fs;

    if let Ok(entries) = fs::read_dir(".") {
        for entry in entries {
            if let Ok(entry) = entry {
                // Here, `entry` is a `DirEntry`.
                if let Ok(metadata) = entry.metadata() {
                    // Now let's show our entry's permissions!
                    println!("{:?}, modified: {:?}", entry.path(), metadata.modified());
                    println!("{:?}, modified: {:?}", entry.path(), metadata.modified());
                } else {
                    println!("Couldn't get metadata for {:?}", entry.path());
                }
            }
        }
    }
}

use anyhow::Context;
use clap::Parser;
use std::path::PathBuf;
use std::{fs, io};

#[derive(clap::Parser, Debug)]
struct Opts {
    #[clap(short, long)]
    msg: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opts = Opts::parse();
    let file = fs::File::open(opts.msg).with_context(|| "Reading msg archive")?;
    let file = io::BufReader::new(file);
    let mut archive = zip::ZipArchive::new(file).with_context(|| "Open msg archive")?;
    for idx in 0..archive.len() {
        let file = archive.by_index(idx).with_context(|| "Reading msg file")?;
        println!("{:?}", String::from_utf8_lossy(file.name_raw()));
    }
    Ok(())
}

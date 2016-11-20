extern crate clap;
extern crate regex;
#[macro_use]
extern crate hogeutilrs;

use std::borrow::{Borrow, Cow};
use std::env;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;


#[derive(Debug)]
struct Cli {
  matchre: Option<regex::Regex>,
  ignore: Arc<Option<regex::Regex>>,
  is_async: bool,
}

impl Cli {
  fn build_cli() -> clap::App<'static, 'static> {
    let program = env::args()
      .nth(0)
      .and_then(|s| {
        PathBuf::from(s)
          .file_stem()
          .map(|s| s.to_string_lossy().into_owned())
      })
      .unwrap();

    use clap::{App, AppSettings, Arg};
    App::new(program)
      .about("find files")
      .version("0.1.0")
      .author("")
      .setting(AppSettings::VersionlessSubcommands)
      .arg(Arg::from_usage("-i --ignore=[IGNORE] 'Ignored pattern'"))
      .arg(Arg::from_usage("-m --matches=[IGNORE] 'pattern to match'"))
      .arg(Arg::from_usage("-a --async 'search asynchronously'"))
  }

  pub fn new() -> Result<Cli, regex::Error> {
    let matches = Self::build_cli().get_matches();

    let ignore: Cow<str> = matches.value_of("ignore")
      .map(Into::into)
      .or(env::var("FILES_IGNORE_PATTERN").ok().map(Into::into))
      .unwrap_or(r#"^(\.git|\.hg|\.svn|_darcs|\.bzr)$"#.into());
    let ignore = if (ignore.borrow() as &str) != "" {
      Some(regex::Regex::new(ignore.borrow())?)
    } else {
      None
    };

    let matchre = match matches.value_of("matches") {
      Some(s) => Some(regex::Regex::new(s)?),
      None => None,
    };

    Ok(Cli {
      matchre: matchre,
      ignore: Arc::new(ignore),
      is_async: matches.is_present("async"),
    })
  }

  pub fn run(&mut self) -> Result<(), String> {
    let root = env::current_dir().unwrap();
    let rx = self.files(&root, self.is_async);

    for entry in rx {
      if let Some(ref m) = self.matchre {
        if !m.is_match(entry.file_name().to_str().unwrap()) {
          continue;
        }
      }
      println!("./{}", entry.path().strip_prefix(&root).unwrap().display());
    }

    Ok(())
  }

  // Scan all files/directories under given directory synchronously
  fn files<P: Into<PathBuf>>(&self, root: P, is_async: bool) -> mpsc::Receiver<fs::DirEntry> {
    let root = root.into();
    let ignore = self.ignore.clone();

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || Self::files_inner(&root, tx, ignore, is_async));

    rx
  }

  fn files_inner(entry: &Path,
                 tx: mpsc::Sender<fs::DirEntry>,
                 ignore: Arc<Option<regex::Regex>>,
                 is_async: bool) {
    if is_match(&entry, ignore.deref()) {
      return;
    }

    if entry.is_dir() {
      for entry in std::fs::read_dir(entry).unwrap() {
        let entry = entry.unwrap();
        if is_async {
          let tx = tx.clone();
          let ignore = ignore.clone();

          thread::spawn(move || {
            Self::files_inner(&entry.path(), tx.clone(), ignore, is_async);
            tx.send(entry).unwrap();
          });
        } else {
          Self::files_inner(&entry.path(), tx.clone(), ignore.clone(), is_async);
          tx.send(entry).unwrap();
        }
      }
    }
  }
}

fn is_match(entry: &Path, pattern: &Option<regex::Regex>) -> bool {
  match *pattern {
    Some(ref pattern) => {
      let filename = entry.file_name()
        .unwrap()
        .to_string_lossy();
      pattern.is_match(filename.borrow())
    }
    None => false,
  }
}

#[derive(Debug)]
enum Error {
  Regex(regex::Error),
  Other(String),
}
def_from! { Error, regex::Error => Regex }
def_from! { Error, String       => Other }

fn _main() -> Result<(), Error> {
  Cli::new()?.run().map_err(Into::into)
}

fn main() {
  _main().unwrap_or_else(|e| panic!("error: {:?}", e));
}

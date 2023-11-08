use std::io::{self,Read, BufReader};
use std::fs::File;
use std::os::unix::prelude::FileExt;
use std::collections::HashMap;
use std::os::unix::fs::{self,PermissionsExt};
use std::path::Path;

use tar::{Archive,EntryType, Entries};

use serde::{Serialize, Deserialize};
use serde::ser::Serializer;
use serde::de::Deserializer;
use serde_with::serde_as;

use log::{info,trace};

use eyre::Result;

use argparse::{ArgumentParser, StoreTrue, Store, List};

struct SimpleLogger;
impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}
static LOGGER: SimpleLogger = SimpleLogger;


#[derive(PartialEq)]
#[derive(Clone)]
#[derive(Serialize,Deserialize,Debug)]
struct MD5Sum([u8; 16]);

impl std::fmt::Display for MD5Sum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = self.0;
        write!(f, "{:x?}", data)
    }
}

#[serde_as]
#[derive(Serialize,Deserialize,Debug)]
#[derive(Clone)]
struct Entry {
    path:      String,
    size:      usize,
    digest:    MD5Sum,
    #[serde(serialize_with = "from_datatype")]
    #[serde(deserialize_with = "to_datatype")]
    data_type: EntryType,
    perm:      u32,
    uid:       u64,
    gid:       u64,
}

fn datatype_to_string(data_type: &EntryType) -> &str {
    match *data_type {
        EntryType::Regular       => "Regular",
        EntryType::Link          => "Link",
        EntryType::Symlink       => "Symlink",
        EntryType::Char          => "Char",
        EntryType::Block         => "Block",
        EntryType::Directory     => "Directory",
        EntryType::Fifo          => "Fifo",
        EntryType::Continuous    => "Continuous",
        EntryType::GNULongName   => "GNULongName",
        EntryType::GNULongLink   => "GNULongLink",
        EntryType::XGlobalHeader => "XGlobalHeader",
        EntryType::XHeader       => "XHeader",

        // if you're a user and happend to stumble upon this, i am sorry
        _                        => panic!("dont pls"),

    }
}

fn datatype_to_i32(data_type: &EntryType) -> i32 {
    match *data_type {
        EntryType::Regular       => 0,
        EntryType::Link          => 1,
        EntryType::Symlink       => 2,
        EntryType::Char          => 3,
        EntryType::Block         => 4,
        EntryType::Directory     => 5,
        EntryType::Fifo          => 6,
        EntryType::Continuous    => 7,
        EntryType::GNULongName   => 8,
        EntryType::GNULongLink   => 9,
        EntryType::XGlobalHeader => 10,
        EntryType::XHeader       => 11,

        // if you're a user and happend to stumble upon this, i am sorry
        _                        => panic!("dont pls"),
    }
}

fn i32_to_datatype(data_type: i32) -> EntryType {
    match data_type {
        0  => EntryType::Regular      ,
        1  => EntryType::Link         ,
        2  => EntryType::Symlink      ,
        3  => EntryType::Char         ,
        4  => EntryType::Block        ,
        5  => EntryType::Directory    ,
        6  => EntryType::Fifo         ,
        7  => EntryType::Continuous   ,
        8  => EntryType::GNULongName  ,
        9  => EntryType::GNULongLink  ,
        10 => EntryType::XGlobalHeader,
        11 => EntryType::XHeader      ,

        // if you're a user and happend to stumble upon this, i am sorry
        _  => panic!("dont pls"),
    }
}

fn from_datatype<S>(data_type: &EntryType, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_i32(datatype_to_i32(data_type))
}

fn to_datatype<'a, D>(deserializer: D) -> Result<EntryType, D::Error>
where
    D: Deserializer<'a>,
{
    Ok(i32_to_datatype(Deserialize::deserialize(deserializer)?))
}

impl Entry {
    fn new<N>(file: &mut tar::Entry<N>) -> Result<(Entry, Vec<u8>)> where
        N: Read
    {
        let mut data = Vec::new();
        let size = file.read_to_end(&mut data).unwrap();
        // Warning: ALWAYS use file.path for file paths instead of file.header.path
        let file_name = String::from(file.path().unwrap().to_str().unwrap());

        let file_type = file.header().entry_type();
        match file_type {
            /*
             * Link behaviour:
             *
             * BE SUPER FUCKING SURE that you handle the following three patterns in file names:
             *
             * 1. File paths starting with './' mean that they're relative to the archive root.
             * 2. File paths starting with '/' mean that they're relative to root. (ignored because
             *    possible security vulnerability)
             * 3. Symlinks destinations (stored in 'data') without a prefix MEAN that their target
             *    is relative TO THE DESTINATION (aka. 'path') OF THE SYMLINK.
             */
            EntryType::Symlink => {
                data = file.link_name_bytes().unwrap().to_vec();
                trace!("Symlink from '{}' to '{}'", file_name, String::from_utf8(data.clone())?);
            },
            EntryType::Link => {
                data = file.link_name_bytes().unwrap().to_vec();
                trace!("Link from '{}' to '{}'", file_name, String::from_utf8(data.clone())?);
            },
            EntryType::Directory => {
                trace!("Directory");
            },
            EntryType::Regular   => {
                trace!("Regular");
            },
            _ => {
                panic!("Unhandled type {}! Path: {}", datatype_to_string(&file_type), file_name);
            },
        }

        Ok((Entry {
            path:      file_name,
            size,
            digest:    MD5Sum(md5::compute(&data).0),
            data_type: file_type,
            perm:      file.header().mode()?,
            uid:       file.header().uid()?,
            gid:       file.header().gid()?,
        }, data))
    }

    fn set_metadata(&self) -> Result<()> {
        let mut perms = std::fs::metadata(&self.path)?.permissions();
        perms.set_mode(self.perm);
        std::fs::set_permissions(&self.path, perms)?;

        // TODO: Fix when unix_chown becomes stable
        //fs::chown(&self.path, file.uid, file.gid);
        Ok(())
    }

    fn write(&self, data: &Vec<u8>) -> Result<()> {
        match self.data_type {
            EntryType::Link => {
                let target = std::str::from_utf8(data)?;
                let _ = std::fs::hard_link(target, &self.path);
            },
            EntryType::Symlink => {
                let target = std::str::from_utf8(data)?;
                let _ = fs::symlink(&self.path, target);
            },
            EntryType::Regular => {
                let f = File::create(&self.path)?;
                let _ = f.write_at(data.as_slice(), 0);
            },
            EntryType::Directory => {
                // hack because it expects that the directory doesnt already exists
                if self.path != "./" {
                    let _ = std::fs::create_dir(&self.path); // Don't use the result, otherwise it
                                                             // can be a unintended panic
                }
            },
            _ => panic!("Unhandled type"),
        }
        Ok(())
    }

    fn remove(&self) -> Result<()> {
        match self.data_type {
            EntryType::Link => {
                std::fs::remove_file(&self.path)?;
            },
            EntryType::Symlink => {
                std::fs::remove_file(&self.path)?;
            },
            EntryType::Regular => {
                std::fs::remove_file(&self.path)?;
            },
            EntryType::Directory => {
                std::fs::remove_dir(&self.path)?;
            },
            _ => panic!("Unhandled type"),
        };
        Ok(())
    }
}

#[serde_as]
#[derive(Serialize,Deserialize,Debug)]
struct FileTree {
    #[serde_as(as = "Vec<(_, _)>")]
    entries: HashMap<String, Entry>,
}

trait CopyTrait {
    fn copy(&self) -> FileTree;
}

impl CopyTrait for FileTree {
    fn copy(&self) -> FileTree {
        FileTree {
            entries: self.entries.clone(),
        }
    }
}

impl FileTree {
    fn new() -> FileTree {
        FileTree { entries: HashMap::new() }
    }

    fn export(&self, file_name: &str) {
        let file = File::create(file_name).unwrap();
        let xz   = xz::write::XzEncoder::new(file, 9);
        let new_tree = self;
        match bincode::serialize_into(xz, &new_tree) {
            Ok(_) => (),
            Err(_) => panic!("Could not write to '{}'", file_name),
        }
    }

    fn import(file_name: &str) -> Result<FileTree> {
        let file = File::open(file_name)?;
        let xz   = xz::bufread::XzDecoder::new(BufReader::new(file));
        let tree = bincode::deserialize_from(xz)?;

        Ok(tree)
    }

    fn apply_tree<N>(&mut self, entries: Entries<N>, ignored: &[String]) -> Result<()> where
        N: Read,
    {
        let mut new_tree = HashMap::new();
        let mut delete_tree = vec![];

        for i in entries {
            let mut i = i?;
            // Warning: ALWAYS use file.path for file paths instead of file.header.path
            let path = i.path().unwrap();

            let mut skip = false;
            for i in ignored {
                if path.starts_with(i) {
                    skip = true;
                }
            }
            if skip {
                continue;
            }

            let (file, data) = Entry::new(&mut i)?;

            info!("Processing {}", &file.path);

            let orig = self.entries.get(&file.path);
            new_tree.insert (
                file.path.clone(),
                file.clone(),
            );
            // does original exist
            if let Some(o) = orig {
                // do comparisons
                if (file.size == o.size) &&
                    (file.digest == o.digest) &&
                    (file.data_type == o.data_type) &&
                    (file.perm      == o.perm) &&
                    (file.uid       == o.uid) &&
                    (file.gid       == o.gid) {
                    // if same, skip
                    continue;
                }

                // if not, replace
                info!("Replace contents of '{}'", file.path);

                file.set_metadata()?;
                file.write(&data)?;

                // replace entry
                self.entries.remove(&file.path);
                self.entries.insert(file.path.to_string(), file);
            } else {
                // it doesn't exist, write a new one
                info!("Create '{}'", file.path);

                file.write(&data)?;
                self.entries.insert(file.path.to_string(), file);
            }
        }

        for k in self.entries.keys() {
            if new_tree.get(k).is_some() {
                continue;
            }
            delete_tree.push(k.clone());
        }

        // files are written chronologically, hence we delete in reverse
        delete_tree.reverse();

        // delete from updated "old" tree
        for i in delete_tree {
            info!("Delete '{}'", i);
            self.entries[&i].remove()?; // remove the file itself
            self.entries.remove(&i);    // the metadata in the hashtable
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut verbose:   bool        = false;
    let mut tarfile:   String      = "".to_string();
    let mut ignored:   Vec<String> = vec![];
    let mut chroot:    String      = "".to_string();
    let mut statefile: String      = ".tarply.state".to_string();

    {
        let mut ap = ArgumentParser::new();
        ap.set_description("Program that only applies changes between two tar backups");
        ap.refer(&mut verbose)
            .add_option(&["-v", "--verbose"], StoreTrue,
            "Be verbose");
        ap.refer(&mut tarfile)
            .add_option(&["-i", "--input"], Store,
            "What tarfile is used as source (-- for stdin)");
        ap.refer(&mut chroot)
            .add_option(&["-C", "--chroot"], Store,
            "What directory is the tarfile being applied to (leave blank for current)");
        ap.refer(&mut ignored)
            .add_option(&["-I", "--ignored"], List, concat!(
                "What locations inside of the tar should be ignored\n",
                "Usage: --ignored file1 file2"));
        ap.refer(&mut statefile)
            .add_option(&["-S", "--state-file"], Store,
            "Change filename of folder state (default '.tarply.state')");
        ap.parse_args_or_exit();
    }

    if verbose {
        log::set_logger(&LOGGER)
            .map(|()| log::set_max_level(log::LevelFilter::Info)).unwrap();
    }

    if tarfile.as_str() == "" {
        println!("You must provide an input!");
        return Ok(());
    }

    let mut a: Archive<Box<dyn Read>> = Archive::new(match tarfile.as_str() {
        "--" => Box::new(io::stdin()),
        _ => {
            Box::new(File::open(tarfile)?)
        },
    });

    // check arguments and grab working dir
    let prefix = match chroot.as_str() {
        "" => std::env::current_dir()?,
        _  => Path::new(chroot.as_str()).to_path_buf(),
    };

    // change working dir
    std::env::set_current_dir(&prefix).expect("Was not able to set --chroot!");

    let mut state_file_path = prefix;
    state_file_path.push(statefile);

    let state_file_path = state_file_path.as_path();

    // create new tree from scratch
    if state_file_path.exists() {
        let mut f = FileTree::import(state_file_path.to_str().unwrap())?;
        f.apply_tree(a.entries().unwrap(), &ignored)?;
        f.export(state_file_path.to_str().unwrap());
    } else {
        let mut f = FileTree::new();
        f.apply_tree(a.entries().unwrap(), &ignored)?;
        f.export(state_file_path.to_str().unwrap());
    }
    Ok(())
}

use crate::card::Card;
use crate::card::CardCollection;
use crate::date::Date;
use crate::parsing;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io::{BufReader, BufWriter};
use std::time::SystemTime;

pub const PARSING_VERSION: u32 = 3;

#[derive(Deserialize, Serialize)]
pub struct CardCache {
    #[serde(default)]
    parsing_version: u32,
    timestamp_cache: HashMap<String, SystemTime>,
    card_cache: HashMap<String, Vec<Card>>,
    #[serde(skip_serializing, skip_deserializing)]
    changed: bool,
}

pub struct File {
    pub path: OsString,
    pub string_path: String,
    pub string_filename: String,
    pub metadata: fs::Metadata,
}

pub fn get_md_files_in_path(path: &OsStr) -> Vec<File> {
    let mut result: Vec<File> = vec![];
    let it_result = fs::read_dir(path);

    if it_result.is_err() {
        return vec![];
    }

    for entry_result in it_result.unwrap() {
        if entry_result.is_err() {
            continue;
        }
        let entry = entry_result.unwrap();
        let filename = entry.file_name();
        let str_filename = filename.to_string_lossy();

        if str_filename.starts_with(".") {
            continue; // skip hidden directories and files
        }

        let file = File {
            path: entry.path().into_os_string(),
            string_path: entry.path().to_string_lossy().to_string(),
            string_filename: str_filename.to_string(),
            metadata: entry.metadata().unwrap(),
        };

        if file.metadata.is_dir() {
            result.extend(get_md_files_in_path(&file.path));
        } else if str_filename.ends_with(".md") {
            result.push(file);
        }
    }

    result
}

#[derive(Debug)]
struct SimpleError {
    details: String,
}

impl SimpleError {
    fn new(msg: &str) -> SimpleError {
        SimpleError {
            details: msg.to_string(),
        }
    }
}

impl fmt::Display for SimpleError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl Error for SimpleError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl CardCache {
    pub fn new() -> CardCache {
        match CardCache::load_from_file() {
            Ok(value) => value,
            Err(_) => CardCache {
                timestamp_cache: HashMap::new(),
                card_cache: HashMap::new(),
                changed: false,
                parsing_version: PARSING_VERSION,
            },
        }
    }

    fn load_from_file() -> Result<CardCache, Box<dyn std::error::Error>> {
        let file = fs::File::open(".tmemocache.json")?;
        let reader = BufReader::new(file);
        let cache: CardCache = serde_json::from_reader(reader)?;

        if cache.parsing_version == PARSING_VERSION {
            Ok(cache)
        } else if cache.parsing_version < PARSING_VERSION {
            Err(Box::new(SimpleError::new(
                "Parsing version was older than current version",
            )))
        } else {
            panic!(
                "Cache had parsing version {} when using tmemo version {}",
                cache.parsing_version, PARSING_VERSION
            );
        }
    }

    pub fn save_to_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        let file = fs::File::create(".tmemocache.json.temp")?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        fs::rename(".tmemocache.json.temp", ".tmemocache.json")?;
        Ok(())
    }

    /// Sees if a file timestamp has changed since it was last read and updates it
    fn has_changed_and_update(&mut self, path: &String, metadata: &fs::Metadata) -> bool {
        let entry = self.timestamp_cache.get(path);
        let modified = metadata.modified().unwrap();
        let result: bool;
        if entry.is_some() {
            result = &modified != entry.unwrap();
        } else {
            result = true;
        }
        if result {
            self.changed = true;
            self.timestamp_cache.insert(path.clone(), modified);
        }
        result
    }

    pub fn get_all_cards_in_work_directory(
        &mut self,
        date_opt: Option<Date>,
    ) -> Result<CardCollection, Box<dyn Error>> {
        let date = match date_opt {
            Some(date) => date,
            _ => Date::now(),
        };

        let mut cards: Vec<Card> = vec![];
        let current_dir = env::current_dir().unwrap();
        let current_path = OsStr::new(&current_dir);
        for entry in get_md_files_in_path(&current_path) {
            let metadata = entry.metadata;
            let path_cards: Vec<Card>;

            if self.has_changed_and_update(&entry.string_path, &metadata) {
                let contents = parsing::read_to_string(&entry.path);
                let heading = entry.string_filename;
                path_cards = parsing::parse_cards(&contents, date, Some(heading));
                self.card_cache
                    .insert(entry.string_path, path_cards.clone());
            } else {
                path_cards = self.card_cache.get(&entry.string_path).unwrap().to_owned();
            }

            cards.extend(path_cards);
        }

        if self.changed {
            self.save_to_file().unwrap();
        }

        CardCollection::from(cards)
    }
}

#[cfg(test)]
mod tests {
    use crate::cardcache::CardCache;
    use crate::date::Date;

    fn date(year: i32, month: u32, day: u32) -> Date {
        Date::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn deck_parsing_files_works() {
        let mut cache = CardCache::new();
        let cards = cache
            .get_all_cards_in_work_directory(Some(date(2024, 1, 1)))
            .unwrap();
        assert_eq!(cards.cards.len(), 11);
    }
}

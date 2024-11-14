use serde_json::Value;
use std::io::{BufReader, BufWriter};

fn migrate_add_version_number(value: &Value) -> Option<Value> {
    let mut output = value.clone();
    if output.get("parsing_version").is_some() {
        return None;
    }
    output.as_object_mut().unwrap().insert(
        "parsing_version".into(),
        serde_json::to_value(2u64).unwrap(),
    );
    Some(output)
}

fn migrate_version_2_to_3(value: &Value) -> Option<Value> {
    let mut output = value.clone();
    let version = output.get("parsing_version")?.as_u64().unwrap();
    if version != 2 {
        return None;
    }
    for card in output.get_mut("cards")?.as_array_mut().unwrap() {
        let content = card.get_mut("content").unwrap();
        let cloze_index = content.get("cloze_index").unwrap().clone();
        let front = content.get("front").unwrap();
        let str: String = front.as_str().unwrap().into();
        if !str.contains('\n')
            || (str.find("\n\n").is_some() && !str.ends_with('\n') && !str.find("\n\n\n").is_some())
        {
            let mut new_str = String::from(str.clone());
            if cloze_index.is_null() {
                new_str.push(' ');
            } else {
                let first_close = str.find("{...}")?;
                let first_newline = str.find("\n\n").unwrap();
                if first_newline < first_close {
                    new_str.insert(first_newline, ' ');
                }
            }
            if str != new_str {
                let front = content.get_mut("front").unwrap();
                *front = Value::String(new_str.clone());
                println!(
                    "Redid card \"{}\" => \"{}\"",
                    str.replace('\n', "\\n"),
                    new_str.replace('\n', "\\n")
                );
            }
        }
    }
    let parsing_ver = output.get_mut("parsing_version")?;
    *parsing_ver = serde_json::to_value(3u64).unwrap();

    Some(output)
}

fn try_migrations(value: &Value) -> Option<Value> {
    if let Some(output) = migrate_add_version_number(value) {
        return Some(output);
    }
    if let Some(output) = migrate_version_2_to_3(value) {
        return Some(output);
    }
    None
}

fn migrate(value: &mut Value) -> bool {
    // Iterate through all migrations until none of them do work
    let mut migration_result = false;
    while let Some(migrated) = try_migrations(value) {
        migration_result = true;
        *value = migrated;
    }
    migration_result
}

pub fn migrate_deck(path: String) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut value: Value;
    {
        let reader = BufReader::new(std::fs::File::open(path.clone())?);
        value = serde_json::from_reader(reader).unwrap();
    }
    if !migrate(&mut value) {
        return Err(String::from("No migration was done").into());
    }
    let mut tmp_path = path.clone();
    tmp_path.push_str(".temp");
    let writer = BufWriter::new(std::fs::File::create(tmp_path.clone())?);
    serde_json::to_writer_pretty(writer, &value)?;
    std::fs::rename(tmp_path, path)?;
    Ok(())
}

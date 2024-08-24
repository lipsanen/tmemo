use crate::card::{Card, CardContent};
use crate::cardcache::get_md_files_in_path;
use crate::date::Date;
use crate::fsrs::FSRSState;
use std::ffi::{OsStr, OsString};
use std::string::String;
use std::vec::Vec;
use std::{env, fs};

#[derive(PartialEq)]
enum MultilineCardState {
    None,
    Front,
    Back,
}

struct Heading {
    pub title: String,
    pub level: u32,
}

// Return None if not a heading,
fn check_markdown_heading(line: &str) -> Option<Heading> {
    for (i, c) in line.chars().enumerate() {
        if c != '#' && i == 0 {
            return None;
        } else if c != '#' && c.is_whitespace() {
            // We have seen both hash and now a non-hash character
            // The title level is the number of hashes seen and the title is the remaining string stripped
            let slice = &line[i..];
            let title = slice.trim().to_string();
            if title.is_empty() {
                return None;
            } else {
                return Some(Heading {
                    title,
                    level: i as u32,
                });
            }
        }
    }

    None
}

fn create_prefix(headings: &Vec<Heading>) -> String {
    headings.iter().fold(String::new(), |a, b| {
        if a.is_empty() {
            b.title.clone()
        } else {
            a + " > " + &b.title
        }
    })
}

struct CardLocationData {
    index: usize,
    len: usize,
}

pub fn read_to_string(filepath: &OsString) -> String {
    fs::read_to_string(filepath)
        .expect(&format!("Was unable to read file {:?}", filepath))
        .replace("\r\n", "\n")
}

pub fn replace_card(
    input: &str,
    heading: Option<String>,
    card: &Card,
    new_card: &Card,
) -> Option<String> {
    let location = find_card(input, card, heading)?;

    let mut output: String = input[0..location.index].to_string();
    output.push_str(&new_card.content.to_string());
    output.push_str(&input[location.index + location.len..]);

    Some(output)
}

pub fn try_replacing_cards(pairs: Vec<(Card, Card)>) {
    let current_dir = env::current_dir().unwrap();
    let current_path = OsStr::new(&current_dir);
    let md_files = get_md_files_in_path(&current_path);

    for pair in pairs {
        let md_filename = pair.0.content.get_md_filename();
        for entry in &md_files {
            if entry.string_filename != md_filename {
                continue;
            }

            let contents = read_to_string(&entry.path);
            let replaced = replace_card(
                &contents,
                Some(entry.string_filename.clone()),
                &pair.0,
                &pair.1,
            )
            .unwrap();
            fs::write(entry.path.clone(), replaced).expect(&format!(
                "Was unable to replace file contents for file {}",
                entry.string_path
            ));
        }
    }
}

fn find_card(input: &str, card: &Card, heading: Option<String>) -> Option<CardLocationData> {
    let mut multiline_state = MultilineCardState::None;
    let mut multiline_front = String::new();
    let mut multiline_back = String::new();
    let mut multiline_start: usize = 0;
    let mut current_line_index: isize;

    let mut headings: Vec<Heading> = match heading {
        Some(value) => vec![Heading {
            title: value.to_owned(),
            level: 0,
        }],
        None => vec![Heading {
            title: "File".to_owned(),
            level: 0,
        }],
    };

    for line in input.lines() {
        unsafe {
            current_line_index = line.as_ptr().offset_from(input.as_ptr());
        }
        match line.find(":: ") {
            Some(index) => {
                let content = CardContent {
                    prefix: create_prefix(&headings),
                    front: line[0..index].to_string(),
                    back: line[index + 3..].to_string(),
                    editable: true,
                    base: None,
                    cloze_index: None,
                };
                if content == card.content && content.back == card.content.back {
                    return Some(CardLocationData {
                        index: current_line_index as usize,
                        len: line.len(),
                    });
                }
            }
            None => (),
        }

        match check_markdown_heading(line) {
            Some(value) => {
                let mut insert_index = 1;
                while insert_index < headings.len() {
                    if headings[insert_index].level < value.level {
                        insert_index += 1;
                    } else {
                        break;
                    }
                }

                headings.insert(insert_index, value);
                headings.truncate(insert_index + 1);
            }
            None => {}
        };

        if line == ":::" {
            match multiline_state {
                MultilineCardState::None => {
                    multiline_start = current_line_index as usize;
                    multiline_state = MultilineCardState::Front
                }
                MultilineCardState::Front => multiline_state = MultilineCardState::Back,
                MultilineCardState::Back => {
                    let content = CardContent {
                        prefix: create_prefix(&headings),
                        front: multiline_front.to_string(),
                        back: multiline_back.to_string(),
                        editable: true,
                        base: None,
                        cloze_index: None,
                    };

                    if content == card.content && content.back == card.content.back {
                        return Some(CardLocationData {
                            index: multiline_start,
                            len: (current_line_index as usize + line.len() - multiline_start),
                        });
                    }

                    multiline_front = String::new();
                    multiline_back = String::new();
                    multiline_state = MultilineCardState::None;
                }
            }
        } else if multiline_state == MultilineCardState::Front {
            multiline_front.push_str(line);
            multiline_front.push('\n');
        } else if multiline_state == MultilineCardState::Back {
            multiline_back.push_str(line);
            multiline_back.push('\n');
        }
    }

    None
}

#[derive(Debug, Clone)]
pub struct QuoteSettings {
    pub words_in_cloze: i32,
    pub words_before_after: i32,
}

#[derive(Debug, Clone)]
pub struct LineSettings {
    pub lines_before_after: i32,
}

#[derive(Debug, Clone)]
pub enum ClozeType {
    TripleBrace,
    TripleParen,
    Lines(LineSettings),
}

pub struct ClozeIterator<'a> {
    pub curr: usize,
    pub input: &'a str,
    pub cloze_type: ClozeType,
    pub quote_words: Vec<&'a str>,
    pub quote_word_index: Option<usize>,
}

pub struct ClozeItem<'a> {
    pub cloze_start: usize,
    pub cloze_end: usize,
    pub before: &'a str,
    pub clozed: &'a str,
    pub after: &'a str,
}

impl<'a> ClozeIterator<'a> {
    pub fn new(cloze_type: ClozeType, input: &'a str) -> ClozeIterator<'a> {
        ClozeIterator {
            curr: 0,
            input,
            cloze_type,
            quote_words: vec![],
            quote_word_index: None,
        }
    }

    fn next_brace(&mut self) -> Option<ClozeItem<'a>> {
        let current_str: &'a str = &self.input[self.curr..];
        let cloze_start = current_str.find("{{{")? + self.curr;
        let current_str: &'a str = &self.input[cloze_start..];
        let cloze_end = current_str.find("}}}")? + cloze_start + 3;
        let end_prev = self.input.get((cloze_end - 4)..);
        let cloze_end_offset = if end_prev.is_some_and(|x| x.chars().next().unwrap() == '\\') {
            4
        } else {
            3
        };

        self.curr = cloze_end;

        Some(ClozeItem {
            cloze_start,
            cloze_end,
            before: &self.input[..cloze_start],
            clozed: &self.input[cloze_start + 3..cloze_end - cloze_end_offset],
            after: &self.input[cloze_end..],
        })
    }

    fn next_paren(&mut self) -> Option<ClozeItem<'a>> {
        let current_str: &'a str = &self.input[self.curr..];
        let cloze_start = current_str.find("(((")? + self.curr;
        let current_str: &'a str = &self.input[cloze_start..];
        let cloze_end = current_str.find(")))")? + cloze_start + 3;
        let end_prev = self.input.get((cloze_end - 4)..);
        let cloze_end_offset = if end_prev.is_some_and(|x| x.chars().next().unwrap() == '\\') {
            4
        } else {
            3
        };

        self.curr = cloze_end;

        Some(ClozeItem {
            cloze_start,
            cloze_end,
            before: &self.input[..cloze_start],
            clozed: &self.input[cloze_start + 3..cloze_end - cloze_end_offset],
            after: "",
        })
    }

    fn build_line_vec(&mut self) {
        let mut working_str = self.input;
        loop {
            let next_word_idx = working_str.find(|c: char| !c.is_whitespace());
            if next_word_idx.is_none() {
                break;
            }
            let next_word_idx = next_word_idx.unwrap();
            working_str = &working_str[next_word_idx..];
            self.quote_words.push(working_str);
            let next_line = working_str.find(|c: char| c == '\n');
            if next_line.is_none() {
                break;
            }
            working_str = &working_str[next_line.unwrap()..];
        }
        self.quote_word_index = Some(0);
    }

    fn get_line_ending(&self, mut index: usize) -> *const u8 {
        if index >= self.quote_words.len() {
            index = self.quote_words.len() - 1;
        }

        let str = &self.quote_words[index];
        let newline_opt = str.find(|x| x == '\n');

        if let Some(opt) = newline_opt {
            unsafe { str.as_ptr().add(opt)}
        } else {
            unsafe { str.as_ptr().add(str.len()) }
        }
    }

    fn next_line(&mut self, settings: LineSettings) -> Option<ClozeItem<'a>> {
        if self.quote_word_index.is_none() {
            self.build_line_vec();
        }

        let index = self.quote_word_index.unwrap();
        if index >= self.quote_words.len() {
            return None;
        }

        let start_index: i32 = (index as i32 - settings.lines_before_after).max(0);
        let before_ptr = self.quote_words[start_index as usize].as_ptr();
        let cloze_ptr = self.quote_words[index].as_ptr();
        let cloze_end_ptr = self.get_line_ending(index);
        let after_ptr = self.get_line_ending(index + settings.lines_before_after as usize);

        let before_index = unsafe { before_ptr.offset_from(self.input.as_ptr()) } as usize;
        let cloze_index = unsafe { cloze_ptr.offset_from(self.input.as_ptr()) } as usize;
        let cloze_end_index = unsafe { cloze_end_ptr.offset_from(self.input.as_ptr()) } as usize;
        let after_index = unsafe { after_ptr.offset_from(self.input.as_ptr()) } as usize;
        self.quote_word_index = Some(index + 1);

        Some(ClozeItem {
            cloze_start: cloze_index,
            cloze_end: after_index,
            before: &self.input[before_index..cloze_index],
            clozed: &self.input[cloze_index..cloze_end_index],
            after: &self.input[cloze_end_index..after_index],
        })
    }
}

impl<'a> Iterator for ClozeIterator<'a> {
    type Item = ClozeItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.cloze_type.clone() {
            ClozeType::TripleBrace => self.next_brace(),
            ClozeType::TripleParen => self.next_paren(),
            ClozeType::Lines(settings) => self.next_line(settings),
        }
    }
}

fn create_cards(
    prefix: String,
    front: String,
    back: String,
    date: Date,
    out_cards: &mut Vec<Card>,
) {
    let card = Card {
        fsrs_state: FSRSState::new(date),
        content: CardContent {
            prefix: prefix.to_string(),
            front: front.to_string(),
            back: back.to_string(),
            editable: true,
            base: None,
            cloze_index: None,
        },
    };

    out_cards.push(card)
}

pub fn parse_cards(input: &str, date: Date, heading: Option<String>) -> Vec<Card> {
    let mut vec: Vec<Card> = vec![];
    let mut multiline_state = MultilineCardState::None;
    let mut multiline_front = String::new();
    let mut multiline_back = String::new();

    let mut headings: Vec<Heading> = match heading {
        Some(value) => vec![Heading {
            title: value.to_owned(),
            level: 0,
        }],
        None => vec![Heading {
            title: "File".to_owned(),
            level: 0,
        }],
    };

    for line in input.lines() {
        if let Some(index) = line.find(":: ") {
            create_cards(
                create_prefix(&headings),
                line[0..index].to_string(),
                line[index + 3..].to_string(),
                date,
                &mut vec,
            );
        }

        if let Some(value) = check_markdown_heading(line) {
            let mut insert_index = 1;
            while insert_index < headings.len() {
                if headings[insert_index].level < value.level {
                    insert_index += 1;
                } else {
                    break;
                }
            }

            headings.insert(insert_index, value);
            headings.truncate(insert_index + 1);
        };

        if line == ":::" {
            match multiline_state {
                MultilineCardState::None => multiline_state = MultilineCardState::Front,
                MultilineCardState::Front => multiline_state = MultilineCardState::Back,
                MultilineCardState::Back => {
                    create_cards(
                        create_prefix(&headings),
                        multiline_front.to_string(),
                        multiline_back.to_string(),
                        date,
                        &mut vec,
                    );
                    multiline_front = String::new();
                    multiline_back = String::new();
                    multiline_state = MultilineCardState::None;
                }
            }
        } else if multiline_state == MultilineCardState::Front {
            multiline_front.push_str(line);
            multiline_front.push('\n');
        } else if multiline_state == MultilineCardState::Back {
            multiline_back.push_str(line);
            multiline_back.push('\n');
        }
    }

    vec
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::*;
    use crate::parsing::CardContent;

    #[test]
    fn inline_parsing_works() {
        let input = "front:: back\n\
                     front2:: back2\n\
                     notparsed::notparsed2\n";
        let cards = parse_cards(input, Date::from_ymd_opt(2024, 1, 1).unwrap(), None);
        assert_eq!(
            cards[0].content,
            CardContent {
                prefix: "File".to_string(),
                front: "front".to_string(),
                back: "back".to_string(),
                editable: true,
                base: None,
                cloze_index: None,
            }
        );
        assert_eq!(
            cards[1].content,
            CardContent {
                prefix: "File".to_string(),
                front: "front2".to_string(),
                back: "back2".to_string(),
                editable: true,
                base: None,
                cloze_index: None,
            }
        );
        assert_eq!(cards.len(), 2);
    }

    #[test]
    fn create_prefix_works() {
        let headings = vec![
            Heading {
                title: "a".to_string(),
                level: 0,
            },
            Heading {
                title: "b".to_string(),
                level: 1,
            },
            Heading {
                title: "c".to_string(),
                level: 2,
            },
        ];
        let prefix = create_prefix(&headings);
        assert_eq!(&prefix, "a > b > c");
    }

    #[test]
    fn headings_work() {
        let input = "# heading\n\
                     ## heading2\n\
                     front:: back\n\
                     front2:: back2\n
                     notparsed::notparsed2\n";
        let cards = parse_cards(input, Date::from_ymd_opt(2024, 1, 1).unwrap(), None);
        assert_eq!(
            cards[0].content,
            CardContent {
                prefix: "File > heading > heading2".to_string(),
                front: "front".to_string(),
                back: "back".to_string(),
                editable: true,
                base: None,
                cloze_index: None,
            }
        );
        assert_eq!(
            cards[1].content,
            CardContent {
                prefix: "File > heading > heading2".to_string(),
                front: "front2".to_string(),
                back: "back2".to_string(),
                editable: true,
                base: None,
                cloze_index: None,
            }
        );
        assert_eq!(cards.len(), 2);
    }

    #[test]
    fn test_md_heading_parsing() {
        let heading = check_markdown_heading("# test").unwrap();
        assert_eq!(&heading.title, "test");
        assert_eq!(heading.level, 1);
        let not_heading = check_markdown_heading(" # test");
        assert!(not_heading.is_none());
        let not_heading2 = check_markdown_heading("#test");
        assert!(not_heading2.is_none());
        let heading_level2 = check_markdown_heading("## test").unwrap();
        assert_eq!(&heading_level2.title, "test");
        assert_eq!(heading_level2.level, 2);
    }

    #[test]
    fn replacing_idempotence() {
        let input = ":::\n\
         front line1\n\
        \tfront line2\n\
        :::\n\
        back line1\n\
        back line2\n\
        \n\
        :::\n
        test1 :: test2\n";

        let cards = parse_cards(input, Date::from_ymd_opt(2024, 1, 1).unwrap(), None);
        let replaced = replace_card(input, None, &cards[0], &cards[0]);
        assert_eq!(replaced.unwrap(), input);
        let replaced = replace_card(input, None, &cards[1], &cards[1]);
        assert_eq!(replaced.unwrap(), input);
    }

    #[test]
    fn replacing() {
        let input = " test1:: test2\n";
        let cards = parse_cards(input, Date::from_ymd_opt(2024, 1, 1).unwrap(), None);
        let mut new_card = cards[0].clone();
        new_card.content.front = " best1".to_string();
        new_card.content.back = "best2".to_string();
        let replaced = replace_card(input, None, &cards[0], &new_card);
        assert_eq!(&replaced.unwrap(), " best1:: best2\n");
    }

    #[test]
    fn replacing2() {
        let input = "\r\n test1:: test2\r\n";
        let cards = parse_cards(input, Date::from_ymd_opt(2024, 1, 1).unwrap(), None);
        let mut new_card = cards[0].clone();
        new_card.content.front = " best1".to_string();
        new_card.content.back = "best2".to_string();
        let replaced = replace_card(input, None, &cards[0], &new_card);
        assert_eq!(&replaced.unwrap(), "\r\n best1:: best2\r\n");
    }

    #[test]
    fn multiline_parsing_works() {
        let input = "askdjasldkjasldkjqweqwee\n\
        :::\n\
        front line1\n\
        front line2\n\
        :::\n\
        back line1\n\
        back line2\n\
        :::\n
        asdlaskjdjlasjda\n\
        ajsdlasjdlkasjda\n\
        qweqwe\n";

        let cards = parse_cards(input, Date::from_ymd_opt(2024, 1, 1).unwrap(), None);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].content.front, "front line1\nfront line2\n");
        assert_eq!(cards[0].content.back, "back line1\nback line2\n");
    }

    #[test]
    fn escaping_cloze_works() {
        let input = "{{{test}\\}}}}";
        let mut iterator = ClozeIterator::new(ClozeType::TripleBrace, input);
        let item = iterator.next().unwrap();
        assert_eq!(item.clozed, "test}")
    }
}

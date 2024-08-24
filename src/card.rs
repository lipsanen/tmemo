use crate::date::Date;
use crate::fsrs::{FSRSState, ReviewLogItem};
use crate::parsing::{ClozeIterator, ClozeType, LineSettings};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::string::String;

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Card {
    pub content: CardContent,
    pub fsrs_state: FSRSState,
}

fn default_surrounding_lines() -> u32 {
    2
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct CardMetadata {
    #[serde(default = "default_surrounding_lines")]
    pub surrounding_lines: u32,
    pub card_type: String,
}

impl PartialOrd for Card {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.content.front.partial_cmp(&other.content.front)
    }
}

impl Ord for Card {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.content.front.cmp(&other.content.front)
    }
}

impl PartialEq for Card {
    fn eq(self: &Self, other: &Self) -> bool {
        self.content.prefix == other.content.prefix && self.content.front == other.content.front
    }
}

impl Eq for Card {}

impl Hash for Card {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.content.prefix.hash(state);
        self.content.front.hash(state);
    }
}

static ESCAPED_STRINGS: &'static [(&str, &str)] = &[
    ("\\n", "[\\]n"),
    ("\\t", "[\\]t"),
    ("\n", "\\n"),
    ("\t", "\\t"),
];

fn convert_to_singleline(mut input: String) -> String {
    for (from, to) in ESCAPED_STRINGS {
        input = input.replace(from, to);
    }

    input
}

fn convert_from_singleline(mut input: String) -> String {
    for (to, from) in ESCAPED_STRINGS.iter().rev() {
        input = input.replace(from, to);
    }

    input
}

impl Card {
    pub fn new() -> Card {
        Card {
            content: CardContent::new(),
            fsrs_state: FSRSState::new(Date { day: 1 }),
        }
    }

    pub fn contains(&self, word: &str) -> bool {
        self.content.front.contains(word)
            || self.content.back.contains(word)
            || self.content.prefix.contains(word)
    }

    pub fn format_to_tsv(&self, current_date: Date) -> String {
        let front = convert_to_singleline(self.content.front.to_string());
        let back = convert_to_singleline(self.content.back.to_string());
        let days_to_review: i32 = self.fsrs_state.review_date.day - current_date.day;
        let date_added = self.fsrs_state.date_added.day - current_date.day;
        let last_review = self.fsrs_state.last_review.day - current_date.day;

        let mut output = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            days_to_review,
            self.fsrs_state.difficulty,
            self.fsrs_state.stability,
            self.content.prefix,
            front,
            back,
            date_added,
            self.fsrs_state.first_review(),
            last_review,
        );

        for log_item in &self.fsrs_state.review_log {
            output.push('\t');
            output.push_str(&log_item.encode().to_string());
        }

        output
    }

    pub fn from_tsv(input: &str, current_date: Date) -> Result<Card, Box<dyn std::error::Error>> {
        let mut card = Card::new();
        let mut iterator = input.split_terminator('\t');
        let days_to_review: i32 = iterator
            .next()
            .ok_or(String::from("missing review_date"))?
            .parse()?;
        card.fsrs_state.buried = false;
        card.fsrs_state.review_date = current_date
            .checked_add_days(days_to_review)
            .ok_or(String::from("invalid review day"))?;
        card.fsrs_state.difficulty = iterator
            .next()
            .ok_or(String::from("missing difficulty"))?
            .parse()?;
        card.fsrs_state.stability = iterator
            .next()
            .ok_or(String::from("missing stability"))?
            .parse()?;
        card.content.prefix = iterator
            .next()
            .ok_or(String::from("missing prefix"))?
            .to_owned();
        card.content.front = convert_from_singleline(
            iterator
                .next()
                .ok_or(String::from("missing front"))?
                .to_owned(),
        );
        card.content.back = convert_from_singleline(
            iterator
                .next()
                .ok_or(String::from("missing back"))?
                .to_owned(),
        );
        let date_added: i32 = iterator
            .next()
            .ok_or(String::from("missing date_added"))?
            .parse()?;
        card.fsrs_state.date_added = current_date.checked_add_days(date_added).unwrap();
        card.fsrs_state.complete_history = iterator
            .next()
            .ok_or(String::from("missing complete_history"))?
            .parse()?;
        let last_review: i32 = iterator
            .next()
            .ok_or(String::from("missing last_review"))?
            .parse()?;
        card.fsrs_state.last_review = current_date.checked_add_days(last_review).unwrap();

        while let Some(value) = iterator.next() {
            let encoded: i64 = value.parse()?;
            let review_item = ReviewLogItem::from(encoded).unwrap();
            card.fsrs_state.review_log.push(review_item);
        }

        Ok(card)
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct CardContent {
    pub prefix: String,
    pub front: String,
    pub back: String,
    pub editable: bool,
    #[serde(skip_serializing, skip_deserializing)]
    pub base: Option<usize>,
    #[serde(default)]
    pub cloze_index: Option<usize>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct CardCollection {
    pub base_cards: Vec<Card>,
    pub cards: Vec<Card>,
}

#[derive(Debug, PartialEq)]
pub enum Editable {
    Editable,
    BaseEditable,
    NotEditable,
}

impl CardContent {
    pub fn new() -> CardContent {
        CardContent {
            prefix: String::new(),
            front: String::new(),
            back: String::new(),
            editable: true,
            base: None,
            cloze_index: None,
        }
    }

    pub fn get_editability(&self) -> Editable {
        if self.editable {
            Editable::Editable
        } else if self.base.is_some() {
            Editable::BaseEditable
        } else {
            Editable::NotEditable
        }
    }

    pub fn get_md_filename(&self) -> &str {
        self.prefix.split('>').next().unwrap().trim_end()
    }

    pub fn to_string(&self) -> String {
        if self.front.find('\n').is_some() {
            format!(":::\n{}:::\n{}:::", self.front, self.back)
        } else {
            format!("{}:: {}", self.front, self.back)
        }
    }

    pub fn get_singleline_front(&self) -> String {
        self.front.replace("\n", "\\n")
    }

    pub fn get_formatted_front(&self) -> String {
        format!("{}\n{}", self.prefix, self.front)
    }

    pub fn key(&self) -> String {
        self.prefix.to_string() + &self.front
    }
}

fn replace_cloze(input: &str, cloze_type: ClozeType) -> String {
    let mut iterator = ClozeIterator::new(cloze_type, input);
    let first = iterator.next();

    if first.is_none() {
        return input.to_string();
    }

    let first = first.unwrap();
    let mut output = first.before.to_string();
    output.push_str(first.clozed);
    let mut prev_cloze_end = first.cloze_end;

    for cloze_item in iterator {
        let in_between = &input[prev_cloze_end..cloze_item.cloze_start];
        prev_cloze_end = cloze_item.cloze_end;
        output.push_str(in_between);
        output.push_str(cloze_item.clozed);
    }

    let in_between = &input[prev_cloze_end..];
    output.push_str(in_between);
    output
}

fn is_at_beginning(input: &str, sub_str: &str) -> bool {
    unsafe { sub_str.as_ptr().byte_offset_from(input.as_ptr()) == 0 }
}

fn is_at_end(input: &str, sub_str: &str) -> bool {
    let start = unsafe { sub_str.as_ptr().byte_offset_from(input.as_ptr()) };
    let end_offset = start as usize + sub_str.len();
    end_offset as usize >= input.len() - 1
}

impl CardCollection {
    fn create_cloze_cards(&mut self, card: &Card, cloze_type: ClozeType) {
        let quote_iterator = ClozeIterator::new(cloze_type, &card.content.back);

        for (index, cloze_item) in quote_iterator.enumerate() {
            let mut cloze_front: String;

            if !is_at_beginning(&card.content.back, cloze_item.before) {
                cloze_front = String::from("...\n");
            } else {
                cloze_front = String::new();
            }
            cloze_front.push_str(cloze_item.before);
            cloze_front.push_str("{...}");
            cloze_front.push_str(cloze_item.after);

            if !is_at_end(&card.content.back, cloze_item.after) {
                cloze_front.push_str("\n...");
            }

            let mut cloze_back: String = String::from("{{{");
            cloze_back.push_str(cloze_item.clozed);
            cloze_back.push_str("}}}");

            let cloze_card = Card {
                fsrs_state: FSRSState::new(card.fsrs_state.date_added),
                content: CardContent {
                    prefix: card.content.prefix.to_string(),
                    front: cloze_front,
                    back: cloze_back,
                    editable: false, // Cloze cards are not editable
                    base: Some(self.base_cards.len()),
                    cloze_index: Some(index),
                },
            };

            self.cards.push(cloze_card);
        }
    }

    fn create_special_cards(
        &mut self,
        card: &Card,
        metadata: CardMetadata,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if metadata.card_type == "line" {
            self.create_cloze_cards(
                card,
                ClozeType::Lines(LineSettings {
                    lines_before_after: metadata.surrounding_lines as i32,
                }),
            );
        } else {
            panic!("unsupported card type {}", metadata.card_type);
        }
        Ok(())
    }

    fn create_basic_cloze_cards(
        &mut self,
        card: Card,
        cloze_type: ClozeType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let iterator = ClozeIterator::new(cloze_type.clone(), &card.content.back);

        for (index, cloze_item) in iterator.enumerate() {
            let mut cloze_front = card.content.front.to_string();
            cloze_front.push_str("\n\n");
            cloze_front.push_str(&replace_cloze(cloze_item.before, cloze_type.clone()));
            cloze_front.push_str("{...}");
            cloze_front.push_str(&replace_cloze(cloze_item.after, cloze_type.clone()));
            let cloze_back = cloze_item.clozed.to_string();

            let cloze_card = Card {
                fsrs_state: FSRSState::new(card.fsrs_state.date_added),
                content: CardContent {
                    prefix: card.content.prefix.to_string(),
                    front: cloze_front,
                    back: cloze_back,
                    editable: false, // Cloze cards are not editable
                    base: Some(self.base_cards.len()),
                    cloze_index: Some(index),
                },
            };

            self.cards.push(cloze_card);
        }

        self.base_cards.push(card);
        Ok(())
    }

    fn create_cards(&mut self, card: Card) -> Result<(), Box<dyn std::error::Error>> {
        let has_triple_braces =
            card.content.back.find("{{{").is_some() && card.content.back.find("}}}").is_some();
        let has_triple_paren =
            card.content.back.find("(((").is_some() && card.content.back.find(")))").is_some();

        if !has_triple_paren && !has_triple_braces {
            self.cards.push(card);
            return Ok(());
        }

        if has_triple_braces {
            self.create_basic_cloze_cards(card, ClozeType::TripleBrace)
        } else {
            self.create_basic_cloze_cards(card, ClozeType::TripleParen)
        }
    }

    pub fn from(cards: Vec<Card>) -> Result<CardCollection, Box<dyn std::error::Error>> {
        let mut collection = CardCollection {
            cards: vec![],
            base_cards: vec![],
        };

        collection.cards.reserve(cards.len());

        for card in cards {
            collection.create_cards(card)?;
        }

        Ok(collection)
    }
}

impl PartialEq for CardContent {
    fn eq(&self, other: &Self) -> bool {
        self.front == other.front && self.prefix == other.prefix
    }
}

impl Eq for CardContent {}

#[cfg(test)]
mod tests {
    use super::{Card, CardCollection, CardContent};
    use crate::date::Date;
    use crate::{fsrs::FSRSState, parsing::parse_cards};

    fn default_date() -> Date {
        Date::from_yo_opt(2024, 1).unwrap()
    }

    //
    #[test]
    fn test_cloze_cards_work2() {
        let cards = vec![Card {
            fsrs_state: FSRSState::new(default_date()),
            content: CardContent {
                prefix: "test".to_string(),
                front: "test".to_string(),
                back: "a reference to an i32 with {{{an explicit lifetime 'a}}}".to_string(),
                editable: false,
                base: None,
                cloze_index: None,
            },
        }];

        let collection = CardCollection::from(cards).unwrap();
        assert_eq!(collection.base_cards.len(), 1);
        assert_eq!(collection.cards.len(), 1);
        assert_eq!(
            &collection.cards[0].content.front,
            "test\n\na reference to an i32 with {...}"
        );
    }

    #[test]
    fn test_cloze_cards_work() {
        let cards = vec![
            Card {
                fsrs_state: FSRSState::new(default_date()),
                content: CardContent {
                    prefix: "test".to_string(),
                    front: "".to_string(),
                    back: "{{{test1}}} {{{test2}}}".to_string(),
                    editable: false,
                    base: None,
                    cloze_index: None,
                },
            },
            Card {
                fsrs_state: FSRSState::new(default_date()),
                content: CardContent {
                    prefix: "test2".to_string(),
                    front: "".to_string(),
                    back: "{{{test1}}} {{{test2}}}".to_string(),
                    editable: false,
                    base: None,
                    cloze_index: None,
                },
            },
        ];

        let collection = CardCollection::from(cards).unwrap();
        assert_eq!(collection.base_cards.len(), 2);
        assert_eq!(collection.cards.len(), 4);
        assert_eq!(&collection.cards[0].content.front, "\n\n{...} test2");
        assert_eq!(&collection.cards[1].content.front, "\n\ntest1 {...}");
        assert_eq!(&collection.cards[2].content.front, "\n\n{...} test2");
        assert_eq!(&collection.cards[3].content.front, "\n\ntest1 {...}");
        assert_eq!(collection.cards[0].content.base.unwrap(), 0);
        assert_eq!(collection.cards[1].content.base.unwrap(), 0);
        assert_eq!(collection.cards[2].content.base.unwrap(), 1);
        assert_eq!(collection.cards[3].content.base.unwrap(), 1);
    }

    #[test]
    fn test_paren_cloze_cards_work() {
        let cards = vec![Card {
            fsrs_state: FSRSState::new(default_date()),
            content: CardContent {
                prefix: "test".to_string(),
                front: "".to_string(),
                back: "xd (((test1))) (((test2)))".to_string(),
                editable: false,
                base: None,
                cloze_index: None,
            },
        }];

        let collection = CardCollection::from(cards).unwrap();
        assert_eq!(collection.base_cards.len(), 1);
        assert_eq!(collection.cards.len(), 2);
        assert_eq!(&collection.cards[0].content.front, "\n\nxd {...}");
        assert_eq!(&collection.cards[1].content.front, "\n\nxd test1 {...}");
        assert_eq!(&collection.cards[0].content.back, "test1");
        assert_eq!(&collection.cards[1].content.back, "test2");
        assert_eq!(collection.cards[0].content.base.unwrap(), 0);
        assert_eq!(collection.cards[1].content.base.unwrap(), 0);
    }

    #[test]
    fn tsv_conversion_works() {
        let mut card = Card::new();
        card.fsrs_state.date_added = Date { day: 5 };
        card.fsrs_state.review_date = Date { day: 999 };
        card.fsrs_state.complete_history = false;
        card.fsrs_state.difficulty = 4.12356;
        card.fsrs_state.stability = 3.2456;
        card.fsrs_state.last_review = Date { day: 995 };
        card.content.front = String::from("test1\n\t\\\\n");
        card.content.back = String::from("test3\n\tasd\\\\t");
        card.content.prefix = String::from("a");
        let current_day = Date { day: 1000 };
        let output = card.format_to_tsv(current_day.clone());
        let parsed = Card::from_tsv(&output, current_day.clone()).unwrap();
        let output2 = parsed.format_to_tsv(current_day.clone());
        assert_eq!(output, output2);
        assert_eq!(parsed.content.front, card.content.front);
        assert_eq!(parsed.content.back, card.content.back);
    }

    #[test]
    fn md_filename_works() {
        let card_content = CardContent {
            prefix: "test.md > asd".to_string(),
            front: "".to_string(),
            back: "".to_string(),
            editable: true,
            base: None,
            cloze_index: None,
        };

        let card_content2 = CardContent {
            prefix: "test2.md".to_string(),
            front: "".to_string(),
            back: "".to_string(),
            editable: true,
            base: None,
            cloze_index: None,
        };

        assert_eq!(card_content.get_md_filename(), "test.md");
        assert_eq!(card_content2.get_md_filename(), "test2.md");
    }
}

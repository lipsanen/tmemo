use crate::card::{BaseCard, Card, CardCollection};
use crate::date::Date;
use crate::fsrs::{FSRSParams, ReviewAnswer, ReviewResult};
use crate::parsing::try_replacing_cards;
use crate::rand::SplitMix64;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, BufWriter};
use std::string::String;
use std::vec::Vec;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Deck {
    pub cards: Vec<Card>,
    pub orphans: Vec<Card>,
    pub track_review_history: bool,
    pub parsing_version: u32,
    pub params: FSRSParams,
    #[serde(default)]
    pub base_cards: Vec<BaseCard>,

    #[serde(skip_serializing, skip_deserializing)]
    pub review_index: Option<usize>,
    #[serde(skip_serializing, skip_deserializing)]
    pub edited_cards: Vec<(Card, Card)>,
    #[serde(skip_serializing, skip_deserializing)]
    pub review_indices: Vec<usize>,
    #[serde(skip_serializing, skip_deserializing)]
    pub review_date: Option<Date>,
}

fn get_indices_to_review(cards: &[Card], date: Date) -> Vec<usize> {
    let mut items: Vec<usize> = Vec::new();

    for (index, card) in cards.iter().enumerate() {
        if date.is_after(&card.fsrs_state.review_date) && !card.fsrs_state.buried {
            items.push(index);
        }
    }

    items
}

fn fix_card_new_lines(mut card: Card) -> Card {
    let front_newlines = card.content.front.find('\n').is_some();
    let back_newlines = card.content.back.find('\n').is_some();

    // last character should be a newline in multiline cards
    if front_newlines || back_newlines {
        let mut front_it = card.content.front.chars().rev();
        match front_it.next() {
            Some('\n') => (),
            _ => card.content.front.push('\n'),
        }

        let mut back_it = card.content.back.chars().rev();
        match back_it.next() {
            Some('\n') => (),
            _ => card.content.back.push('\n'),
        }
    }

    card
}

impl Deck {
    pub fn new() -> Deck {
        Deck {
            cards: vec![],
            orphans: vec![],
            review_indices: vec![],
            review_index: None,
            review_date: None,
            edited_cards: vec![],
            base_cards: vec![],
            track_review_history: false,
            params: FSRSParams::new(),
            parsing_version: crate::cardcache::PARSING_VERSION,
        }
    }

    pub fn stop_review(&mut self) {
        self.review_indices.clear();
        self.review_index = None;
        self.review_date = None;
    }

    pub fn load_from_tsv(input: String) -> Result<Deck, Box<dyn std::error::Error>> {
        let mut deck = Deck::new();
        let date = Date::now();

        for line in input.lines() {
            let card = Card::from_tsv(line, date.clone())?;
            deck.cards.push(card);
        }

        Ok(deck)
    }

    pub fn load_from_file() -> Result<Deck, Box<dyn std::error::Error>> {
        let file = fs::File::open("tmemodeck.json")?;
        let reader = BufReader::new(file);
        let d = serde_json::from_reader(reader)?;
        Ok(d)
    }

    pub fn save_to_file(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let edited: Vec<(Card, Card)> = self.edited_cards.drain(0..).collect();
        try_replacing_cards(edited);

        let file = fs::File::create("tmemodeck.json.temp")?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        fs::rename("tmemodeck.json.temp", "tmemodeck.json")?;
        Ok(())
    }

    pub fn edit_card(&mut self, new_card: Card, card_index: usize) {
        self.edit_base_card(self.cards[card_index].content.base, new_card);
    }

    // fn edit_card_internal(&mut self, idx: usize, new_card: Card) {
    //     let base_card_index = self.cards[idx].content.base;
    //     self.edit_base_card(base_card_index, new_card);
    // }

    pub fn edit_base_card(&mut self, base_card_index: usize, mut new_card: Card) {
        new_card = fix_card_new_lines(new_card);
        let cards = vec![new_card.clone()];
        let collection = CardCollection::from(cards).unwrap();
        self.edited_cards
            .push((self.base_cards[base_card_index].clone().into(), new_card.clone()));
        self.base_cards[base_card_index] = new_card.into();

        // Fill the cards in order
        'outer: for clozed_card in collection.cards {
            for card in self.cards.iter_mut() {
                let parent = card.content.base;
                if parent == base_card_index && card.content.child_index == clozed_card.content.child_index
                {
                    card.content = clozed_card.content.clone();
                    card.content.base = base_card_index;
                    continue 'outer;
                }
            }
            self.cards.push(clozed_card);
        }
    }

    pub fn get_review_card(&self) -> Option<&Card> {
        if let Some(index) = self.review_index {
            let card_index = self.review_indices[index];
            Some(&self.cards[card_index])
        } else {
            None
        }
    }

    pub fn review_card(self: &mut Self, answer: ReviewAnswer, generator: &mut SplitMix64) {
        let review_index = self.review_index.unwrap();
        let card_index = self.review_indices[review_index];
        let result = self.cards[card_index].fsrs_state.review_with_rng(
            answer,
            &self.review_date.unwrap(),
            self.track_review_history,
            generator,
            &self.params,
        );
        match result {
            ReviewResult::Discard => {
                let card : &Card = &self.cards[card_index];
                if card.fsrs_state.stability > 2.0 {
                    let offset = self.card_review_offset(card.fsrs_state.review_date);
                    self.cards[card_index].fsrs_state.review_date.day += offset;
                }

                self.review_indices.remove(review_index)
            },
            _ => 0,
        };
        self.gen_review_index(generator);
    }

    fn gen_review_index(self: &mut Self, generator: &mut SplitMix64) {
        let count = self.review_indices.len();
        if count == 0 {
            self.review_index = None;
        } else if count == 1 {
            self.review_index = Some(0);
        } else {
            let mut new_index = generator.next_rand() as usize % self.review_indices.len();

            if self.review_index.is_some() {
                while new_index == self.review_index.unwrap() {
                    new_index += 1;
                    new_index %= self.review_indices.len();
                }
            }

            self.review_index = Some(new_index);
        }
    }

    pub fn start_random_review(
        &mut self,
        date: Date,
        generator: &mut SplitMix64,
        mut review_count: usize,
    ) {
        self.review_indices.clear();
        self.review_indices.reserve(review_count);
        review_count = review_count.min(self.cards.len());
        for _i in 0..review_count {
            let mut new_index = generator.next_rand() as usize % self.cards.len();

            while self.review_indices.contains(&new_index)
                || self.cards[new_index].fsrs_state.buried
            {
                new_index += 1;
                new_index %= self.cards.len();
            }
            self.review_indices.push(new_index);
        }
        self.review_date = Some(date);
        self.gen_review_index(generator);
    }

    pub fn start_all_review(&mut self, date: Date, generator: &mut SplitMix64) {
        self.review_indices.clear();
        self.review_indices.reserve(self.cards.len());
        for i in 0..self.cards.len() {
            if !self.cards[i].fsrs_state.buried {
                self.review_indices.push(i);
            }
        }
        self.review_date = Some(date);
        self.gen_review_index(generator);
    }

    pub fn start_review(self: &mut Self, date: Date, generator: &mut SplitMix64) {
        self.review_indices = get_indices_to_review(&self.cards, date);
        self.review_date = Some(date);
        self.gen_review_index(generator);
    }

    pub fn cards_to_review_count(&self, date: Date) -> usize {
        get_indices_to_review(&self.cards, date).len()
    }

    pub fn active_review_count(self: &Self) -> usize {
        self.review_indices.len()
    }

    fn card_review_offset(&self, day: Date) -> i32 {
        // If the current day is a local maxima, move the review to another day
        let mut yesterday_count = 0;
        let mut today_count = 0;
        let mut tomorrow_count = 0;
        for card in &self.cards {
            if card.fsrs_state.review_date == day {
                today_count += 1;
            } else if card.fsrs_state.review_date.day + 1 == day.day {
                yesterday_count += 1;
            } else if card.fsrs_state.review_date.day - 1 == day.day {
                tomorrow_count += 1;
            }
        }

        if today_count > yesterday_count && tomorrow_count >= yesterday_count {
            -1
        } else if today_count > tomorrow_count && yesterday_count > tomorrow_count {
            1
        } else {
            0
        }
    }

    pub fn random_reschedule_fractional(&mut self, frac_diff: f64, generator: &mut SplitMix64) {
        for card in self.cards.iter_mut() {
            if card.fsrs_state.buried {
                continue;
            }

            let value = generator.next_float(1.0 - frac_diff, 1.0 + frac_diff);
            let rng_stability =
                (card.fsrs_state.review_date.day - card.fsrs_state.last_review.day) as f64 * value;
            let no_days = rng_stability.round().max(1.0) as i32;
            card.fsrs_state.review_date = card
                .fsrs_state
                .last_review
                .checked_add_days(no_days)
                .unwrap();
        }
    }

    pub fn reschedule(&mut self, first_day: Date, days: i32, mut max_cards_per_day: usize) {
        let mut total_cards_for_days = 0.0;
        let mut indices: Vec<usize> = Vec::new();
        for (index, card) in self.cards.iter().enumerate() {
            if !card.fsrs_state.buried && card.fsrs_state.review_date.day - first_day.day < days {
                total_cards_for_days += 1.0;
                indices.push(index);
            }
        }

        let cards_per_day = (total_cards_for_days / days as f64).ceil() as usize;
        max_cards_per_day = max_cards_per_day.max(cards_per_day);
        let mut vec_counts = vec![0; days as usize];

        let mut max_diff: i32 = 0;
        loop {
            if indices.is_empty() {
                break;
            }

            indices = indices
                .into_iter()
                .filter(|index| {
                    let card_ref: &mut Card = self.cards.get_mut(index.clone()).unwrap();

                    for i in 0..max_diff * 2 + 1 {
                        let day = card_ref
                            .fsrs_state
                            .review_date
                            .checked_add_days(-max_diff + i)
                            .unwrap();
                        let day_idx: i32 = day.day - first_day.day;
                        if day_idx >= 0
                            && day_idx < days
                            && vec_counts[day_idx as usize] < max_cards_per_day
                        {
                            card_ref.fsrs_state.review_date = day;
                            vec_counts[day_idx as usize] += 1;
                            return false;
                        }
                    }

                    true
                })
                .collect();
            max_diff += 1;
        }
    }

    pub fn replace_cards(
        &mut self,
        collection: CardCollection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut map: HashMap<String, Card> = HashMap::new();
        let new_cards = collection.cards;
        self.base_cards = collection.base_cards;
        map.reserve(new_cards.len());

        for card in new_cards {
            let key = card.content.key();

            if map.contains_key(&key) {
                // duplicate detected, set the previous card as not editable
                map.get_mut(&key).unwrap().content.editable = false;
            } else {
                map.insert(key.clone(), card);
            }
        }

        let mut updated_cards: Vec<Card> = Vec::new();
        let old_cards = self.cards.drain(0..);

        for mut card in old_cards {
            let key = card.content.key();
            if map.contains_key(&key) {
                let new_entry = map.remove(&key).unwrap();
                card.content = new_entry.content;
                updated_cards.push(card);
            } else {
                self.orphans.push(card);
            }
        }

        for (_, mut card) in map {
            for i in 0..self.orphans.len() {
                if &self.orphans[i].content.front == &card.content.front {
                    let orphan = self.orphans.remove(i);
                    card.fsrs_state = orphan.fsrs_state;
                    break;
                }
            }

            updated_cards.push(card);
        }

        self.cards = updated_cards;
        self.cards.sort();
        Ok(())
    }

    pub fn print_card_data(&self) {
        let current_date = Date::now();
        for card in &self.cards {
            if card.fsrs_state.buried {
                continue;
            }
            println!("{}", card.format_to_tsv(current_date));
        }
    }

    pub fn get_accuracy_data(&self, current_day: Date) -> BTreeMap<i32, (u64, u64)> {
        let mut map: BTreeMap<i32, (u64, u64)> = BTreeMap::new();
        map.insert(-1, (0, 0));

        for card in &self.cards {
            let mut prev_day = 0;
            for review_log in &card.fsrs_state.review_log {
                if review_log.day.day == prev_day {
                    // Only the first answer of the day is calculated in accuracy
                    continue;
                }
                prev_day = review_log.day.day;
                let day = current_day.day - review_log.day.day;
                let was_correct: bool = review_log.answer == ReviewAnswer::Again;
                let total_entry = map.get_mut(&-1).unwrap();
                total_entry.1 += 1;
                let mut correct: u64 = if was_correct {
                    0
                } else {
                    total_entry.0 += 1;
                    1
                };
                let current_entry = map.get_mut(&day);
                let mut total: u64 = 1;

                match current_entry {
                    Some((prev_correct, prev_total)) => {
                        correct += *prev_correct;
                        total += *prev_total;
                    }
                    None => {}
                }

                map.insert(day, (correct, total));
            }
        }

        map
    }

    pub fn find_cards(&self, search_input: String) -> Vec<usize> {
        let mut card_indices: Vec<usize> = (0..self.cards.len()).collect();
        let words = search_input.split_whitespace();

        for word in words {
            card_indices = card_indices
                .into_iter()
                .filter(|index| self.cards[*index].contains(word))
                .collect();
        }

        card_indices
    }
}

#[cfg(test)]
mod tests {
    use crate::card::*;
    use crate::date::*;
    use crate::deck::*;
    use crate::fsrs::FSRSState;
    use crate::fsrs::ReviewAnswer;

    fn date(year: i32, month: u32, day: u32) -> Date {
        Date::from_ymd_opt(year, month, day).unwrap()
    }

    fn default_date() -> Date {
        Date::from_yo_opt(2024, 1).unwrap()
    }

    fn new_card(front: &str) -> Card {
        Card {
            content: CardContent {
                prefix: String::new(),
                front: front.to_string(),
                back: String::new(),
                editable: true,
                base: 0,
                child_index: 0,
            },
            fsrs_state: FSRSState::new(default_date()),
        }
    }

    fn new_card_with_date(front: &str, date: Date) -> Card {
        Card {
            content: CardContent {
                prefix: String::new(),
                front: front.to_owned(),
                back: String::new(),
                editable: true,
                base: 0,
                child_index: 0,
            },
            fsrs_state: FSRSState::new(date),
        }
    }

    fn new_card_with_back(front: &str, back: &str) -> Card {
        Card {
            content: CardContent {
                prefix: String::new(),
                front: front.to_owned(),
                back: back.to_owned(),
                editable: true,
                base: 0,
                child_index: 0,
            },
            fsrs_state: FSRSState::new(default_date()),
        }
    }

    #[test]
    fn orphan_linking_works() {
        let mut deck = Deck::new();
        let cards1 = vec![
            new_card_with_date("front1", date(2024, 1, 1)),
            new_card_with_date("front2", date(2024, 1, 1)),
        ];
        let mut cards2 = vec![
            new_card_with_date("front1", date(2024, 1, 2)),
            new_card_with_date("front2", date(2024, 1, 2)),
        ];
        cards2[0].content.prefix = "a".to_string();
        cards2[1].content.prefix = "b".to_string();
        let collection1 = CardCollection::from(cards1).unwrap();
        let collection2 = CardCollection::from(cards2).unwrap();

        let _ = deck.replace_cards(collection1);
        let _ = deck.replace_cards(collection2);
        assert!(deck.orphans.is_empty());
        // Original fsrs_state along with dates added should be preserved
        assert_eq!(deck.cards[0].fsrs_state.date_added, date(2024, 1, 1));
        assert_eq!(deck.cards[1].fsrs_state.date_added, date(2024, 1, 1));
    }

    #[test]
    fn cloze_card_editing_works() {
        let cloze_cards = vec![new_card_with_back("front1", "{{{back1}}} {{{back2}}}")];
        let collection = CardCollection::from(cloze_cards).unwrap();
        let mut deck = Deck::new();
        let _ = deck.replace_cards(collection);

        assert_eq!(deck.cards.len(), 2);
        deck.edit_base_card(0, new_card_with_back("front1", "{{{back3}}} {{{back4}}}"));
        assert_eq!(deck.cards[0].content.front, "front1\n\nback3 {...}");
        assert_eq!(deck.cards[1].content.front, "front1\n\n{...} back4");
        deck.edit_base_card(
            0,
            new_card_with_back("front1", "{{{back3}}} {{{back4}}} {{{back5}}}"),
        );
        assert_eq!(deck.cards.len(), 3);
        assert_eq!(deck.cards[0].content.front, "front1\n\nback3 {...} back5");
        assert_eq!(deck.cards[1].content.front, "front1\n\n{...} back4 back5");
        assert_eq!(deck.cards[2].content.front, "front1\n\nback3 back4 {...}");
    }

    #[test]
    fn reschedule_works() {
        let mut deck = Deck::new();
        let mut vec: Vec<Card> = vec![];
        let start_day = date(2024, 1, 1);

        for i in 0..50 {
            vec.push(new_card_with_date(&format!("test{}", i), start_day.clone()));
        }
        let collection = CardCollection::from(vec).unwrap();
        let _ = deck.replace_cards(collection);
        deck.reschedule(date(2024, 1, 1), 2, 1);
        assert_eq!(deck.cards[0].fsrs_state.review_date, start_day);
        assert_eq!(
            deck.cards[25].fsrs_state.review_date,
            start_day.checked_add_days(1).unwrap()
        );
    }

    #[test]
    fn review_works() {
        let mut deck = Deck::new();
        let vec = vec![
            new_card_with_date("card1", date(2024, 1, 1)),
            new_card_with_date("card2", date(2024, 2, 1)),
            new_card_with_date("card3", date(2024, 1, 15)),
        ];

        let mut generator = SplitMix64::from_seed(42);
        let collection = CardCollection::from(vec).unwrap();

        let _ = deck.replace_cards(collection);
        deck.start_review(date(2024, 1, 15), &mut generator);
        for i in 0..100 {
            assert_eq!(deck.active_review_count(), 2);
            if i % 2 == 0 {
                assert_eq!(deck.get_review_card().unwrap().content.front, "card3");
            } else {
                assert_eq!(deck.get_review_card().unwrap().content.front, "card1");
            }
            deck.review_card(ReviewAnswer::Again, &mut generator);
        }
        assert_eq!(deck.get_review_card().unwrap().content.front, "card3");
        deck.review_card(ReviewAnswer::Good, &mut generator);
        assert_eq!(deck.active_review_count(), 1);
        assert_eq!(deck.get_review_card().unwrap().content.front, "card1");
        deck.review_card(ReviewAnswer::Good, &mut generator);
        assert_eq!(deck.active_review_count(), 0);
        assert_eq!(deck.get_review_card(), None);
        deck.start_review(date(2024, 1, 15), &mut generator);
        assert_eq!(deck.active_review_count(), 0);
        assert_eq!(deck.cards[0].fsrs_state.review_date, date(2024, 1, 16));
        assert_eq!(deck.cards[1].fsrs_state.review_date, date(2024, 2, 1));
        assert_eq!(deck.cards[2].fsrs_state.review_date, date(2024, 1, 16));
    }

    #[test]
    fn getting_review_cards_works() {
        let vec = vec![
            Card {
                content: CardContent::new(),
                fsrs_state: FSRSState::new(date(2024, 1, 1)),
            },
            Card {
                content: CardContent::new(),
                fsrs_state: FSRSState::new(date(2024, 2, 1)),
            },
            Card {
                content: CardContent::new(),
                fsrs_state: FSRSState::new(date(2024, 1, 15)),
            },
        ];

        let indices = get_indices_to_review(&vec, date(2024, 1, 15));
        assert_eq!(indices[0], 0);
        assert_eq!(indices[1], 2);
    }

    #[test]
    fn update_works() {
        let cards: Vec<Card> = (0..100).map(|x| new_card(&format!("{}", x))).collect();
        let collection = CardCollection::from(cards).unwrap();

        let mut deck: Deck = Deck::new();
        let _ = deck.replace_cards(collection.clone());
        assert_eq!(deck.cards.len(), 100);
        let _ = deck.replace_cards(collection);
        assert_eq!(deck.cards.len(), 100);
        assert_eq!(deck.orphans.len(), 0);
    }

    #[test]
    fn random_review_works() {
        let cards: Vec<Card> = (0..100).map(|x| new_card(&format!("{}", x))).collect();
        let mut generator = SplitMix64::from_seed(0);

        let mut deck: Deck = Deck::new();
        let _ = deck.replace_cards(CardCollection::from(cards.clone()).unwrap());
        deck.start_random_review(default_date(), &mut generator, 20);
        assert!(deck.review_index.is_some());
        assert_eq!(deck.active_review_count(), 20);
    }

    #[test]
    fn duplicates_error() {
        let cards: Vec<Card> = vec![new_card("front"), new_card("front")];

        let mut deck: Deck = Deck::new();
        let result = deck.replace_cards(CardCollection::from(cards).unwrap());
        assert!(result.is_ok());
        assert_eq!(deck.cards.len(), 1);
        assert_eq!(
            deck.cards[0].content.get_editability(),
            Editable::NotEditable
        );
    }
}

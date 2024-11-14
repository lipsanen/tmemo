use crate::fsrs::ReviewAnswer;
use crate::rand::SplitMix64;
use crate::{
    card::{Card, CardCollection, Editable},
    cardcache::CardCache,
    date::Date,
    deck::{self, Deck},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use serde::{Deserialize, Serialize};
use std::io::BufReader;
use std::vec;

#[derive(Deserialize, Serialize)]
pub struct ApplicationState {
    pub current_state: TMemoInternalState,
    undo_history: Vec<TmemoStateAction>,
    undo_startpoint: TMemoInternalState,
    undo_index: usize,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub enum EditMode {
    None,
    EditFront,
    EditBack,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct FindViewState {
    pub search_input: String,
    pub search_results: Vec<usize>,
    pub search_index: usize,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct TMemoInternalState {
    pub output_text: String,
    pub main_index: u32,
    pub review_show_back: bool,
    pub view: TMemoStateView,
    pub wants_to_quit: bool,
    pub deck: deck::Deck,
    pub rng: SplitMix64,
    pub current_card: Option<Card>,
    pub edit_index: Option<usize>,
    pub edit_mode: EditMode,
    pub edit_return_view: TMemoStateView,
    pub find_state: FindViewState,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub enum TMemoStateView {
    Main,
    Review,
    Hotkeys,
    Find,
    Edit,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub enum TmemoStateAction {
    Up,
    Down,
    Quit,
    Undo,
    Redo,
    ReplaceCards(CardCollection),
    FetchAllCards,
    LoadFromJson,
    LoadFromStdin,
    SaveToJson,
    StartReview,
    StartRandomReview,
    StartAllReview,
    ExitReview,
    StartHotkeys,
    ShowBack,
    CardResponse(ReviewAnswer),
    LoadDeck(Box<Deck>),
    StartEdit(EditMode),
    StartBaseEdit(EditMode),
    FinishEdit(bool),
    RawKey(char, KeyModifiers),
    RawBackspace,
    CursorMove(i32),
    Seed(u64),
    DumpApplicationState,
    LoadApplicationState(String),
    EnterView(TMemoStateView),
    StartFindEdit,
    ToggleClozeType,
}

impl ApplicationState {
    pub fn new() -> ApplicationState {
        ApplicationState {
            current_state: TMemoInternalState::new(),
            undo_history: vec![],
            undo_startpoint: TMemoInternalState::new(),
            undo_index: 0,
        }
    }

    fn redo(self: &mut ApplicationState) {
        if self.undo_index == self.undo_history.len() {
            return;
        }

        self.current_state
            .process(&self.undo_history[self.undo_index]);
        self.undo_index += 1;
    }

    fn undo(self: &mut ApplicationState) {
        if self.undo_index == 0 || self.undo_history.is_empty() {
            return;
        }

        let target_index = self.undo_index - 1;
        self.undo_index = 0;
        self.current_state = self.undo_startpoint.clone();

        for _i in 0..target_index {
            self.redo();
        }
    }

    pub fn load_from_stdin(&mut self) {
        self.process(TmemoStateAction::LoadFromStdin);
    }

    pub fn load_from_file(&mut self) {
        self.process(TmemoStateAction::LoadFromJson);
        self.process(TmemoStateAction::FetchAllCards);
    }

    pub fn load_from_statefile(&mut self, filepath: String) {
        self.process(TmemoStateAction::LoadApplicationState(filepath));
    }

    pub fn process(self: &mut ApplicationState, action: TmemoStateAction) {
        let processed = match &action {
            TmemoStateAction::Undo => {
                self.undo();
                true
            }
            TmemoStateAction::Redo => {
                self.redo();
                true
            }
            TmemoStateAction::DumpApplicationState => {
                let file = std::fs::File::create("tmemostate.json.temp").unwrap();
                let writer = std::io::BufWriter::new(file);
                serde_json::to_writer_pretty(writer, self).unwrap();
                std::fs::rename("tmemostate.json.temp", "tmemostate.json").unwrap();
                true
            }
            TmemoStateAction::LoadApplicationState(filepath) => {
                let file = std::fs::File::open(filepath).unwrap();
                let reader = BufReader::new(file);
                *self = serde_json::from_reader(reader).unwrap();
                true
            }
            TmemoStateAction::FetchAllCards => {
                let mut cache = CardCache::new();
                let cards = cache
                    .get_all_cards_in_work_directory(None)
                    .expect("Error fetching all cards from working directory");
                let action = TmemoStateAction::ReplaceCards(cards);
                self.process(action);
                true
            }
            TmemoStateAction::LoadFromStdin => {
                let input = std::io::read_to_string(std::io::stdin()).unwrap();
                let deck = Deck::load_from_tsv(input).unwrap();
                self.process(TmemoStateAction::LoadDeck(Box::new(deck)));
                true
            }
            TmemoStateAction::LoadFromJson => {
                let deck = Deck::load_from_file().unwrap();
                self.process(TmemoStateAction::LoadDeck(Box::new(deck)));
                true
            }
            TmemoStateAction::SaveToJson => {
                self.current_state.deck.save_to_file().unwrap();
                true
            }
            _ => false,
        };

        if !processed {
            let success = self.current_state.process(&action);

            if success {
                if self.undo_index < self.undo_history.len() {
                    self.undo_history.truncate(self.undo_index);
                }
                self.undo_history.push(action);
                self.undo_index += 1;
            }
        }
    }
}

impl FindViewState {
    pub fn new() -> FindViewState {
        FindViewState {
            search_input: String::new(),
            search_results: vec![],
            search_index: 0,
        }
    }
}

impl TMemoInternalState {
    pub fn new() -> TMemoInternalState {
        TMemoInternalState {
            output_text: "Welcome to tmemo!".to_owned(),
            main_index: 0,
            review_show_back: false,
            wants_to_quit: false,
            view: TMemoStateView::Main,
            deck: deck::Deck::new(),
            rng: SplitMix64::from_seed(42),
            current_card: None,
            edit_mode: EditMode::None,
            edit_index: None,
            edit_return_view: TMemoStateView::Review,
            find_state: FindViewState::new(),
        }
    }

    fn set_review_card(&mut self) {
        self.current_card = match self.deck.get_review_card().as_ref() {
            Some(&card) => Some(card.clone()),
            None => None,
        };
    }

    fn process_main_view(self: &mut TMemoInternalState, action: &TmemoStateAction) -> bool {
        match action {
            TmemoStateAction::LoadDeck(deck) => {
                self.deck = deck.as_ref().clone();
                let count: usize = self
                    .deck
                    .cards
                    .iter()
                    .filter(|x| !x.fsrs_state.buried)
                    .count();
                self.output_text = format!("Loaded deck with {count} cards");
                true
            }
            TmemoStateAction::ReplaceCards(cards) => {
                match self.deck.replace_cards(cards.clone(), Date::now()) {
                    Ok(()) => {
                        let count = self
                            .deck
                            .cards
                            .iter()
                            .filter(|x| !x.fsrs_state.buried)
                            .count();
                        self.output_text = format!("Loaded deck with {count} cards");
                    }
                    Err(err) => {
                        self.output_text = format!("Failed to update cards {}", err);
                    }
                }
                true
            }
            TmemoStateAction::StartAllReview => {
                self.view = TMemoStateView::Review;
                self.review_show_back = false;
                self.deck.start_all_review(Date::now(), &mut self.rng);
                self.set_review_card();
                true
            }
            TmemoStateAction::StartReview => {
                self.view = TMemoStateView::Review;
                self.review_show_back = false;
                self.deck.start_review(Date::now(), &mut self.rng);
                self.set_review_card();
                true
            }
            TmemoStateAction::StartRandomReview => {
                self.view = TMemoStateView::Review;
                self.review_show_back = false;
                self.deck
                    .start_random_review(Date::now(), &mut self.rng, 17);
                self.set_review_card();
                true
            }
            TmemoStateAction::StartHotkeys => {
                self.view = TMemoStateView::Hotkeys;
                true
            }
            TmemoStateAction::Up => {
                if self.main_index == 0 {
                    false
                } else {
                    self.main_index -= 1;
                    true
                }
            }
            TmemoStateAction::Down => {
                if self.main_index == 3 {
                    false
                } else {
                    self.main_index += 1;
                    true
                }
            }
            _ => false,
        }
    }

    fn handle_edit_key(&mut self, c: char, modifiers: KeyModifiers) -> bool {
        let added: String = match modifiers {
            KeyModifiers::SHIFT => c.to_uppercase().to_string(),
            _ => c.to_string(),
        };

        let str: &mut String;
        if self.edit_mode == EditMode::EditFront {
            str = &mut self.current_card.as_mut().unwrap().content.front;
        } else {
            str = &mut self.current_card.as_mut().unwrap().content.back;
        }

        if !str.is_empty() {
            let index = match self.edit_index {
                None => str.len() + 1,
                Some(idx) => idx,
            };
            let mut new_str: String = String::new();
            if index <= 1 {
                new_str.push_str(&added);
                new_str.push_str(str)
            } else {
                let part2: String = str.chars().skip(index - 1).collect();
                new_str = str.chars().take(index - 1).collect();
                new_str.push_str(&added);
                new_str.push_str(&part2);
            }
            self.edit_index = Some(index + 1);
            *str = new_str;
        } else {
            *str = added;
        }

        return true;
    }

    fn process_edit(self: &mut TMemoInternalState, action: &TmemoStateAction) -> bool {
        match action {
            TmemoStateAction::FinishEdit(result) => {
                self.edit_mode = EditMode::None;
                self.view = self.edit_return_view.clone();
                if !result {
                    self.set_review_card();
                } else {
                    let new_card = self.current_card.as_mut().unwrap().clone();
                    let index: usize;
                    if self.edit_return_view == TMemoStateView::Review {
                        index = self.deck.review_indices[self.deck.review_index.unwrap()];
                    } else {
                        index = self.find_state.search_results[self.find_state.search_index];
                    }
                    self.deck.edit_card(new_card, index);
                    self.set_review_card();
                }
                true
            }
            TmemoStateAction::StartEdit(mode) => {
                if self
                    .current_card
                    .as_ref()
                    .unwrap()
                    .content
                    .get_editability()
                    != Editable::Editable
                {
                    panic!("Tried to edit an uneditable card");
                }
                self.edit_mode = mode.clone();
                self.edit_index = None;
                true
            }
            TmemoStateAction::CursorMove(m) => {
                let text: &str = match self.edit_mode {
                    EditMode::EditFront => &self.current_card.as_mut().unwrap().content.front,
                    EditMode::EditBack => &self.current_card.as_mut().unwrap().content.back,
                    EditMode::None => panic!("not in edit mode!"),
                };

                if text.len() == 0 {
                    self.edit_index = None;
                    return true;
                }

                let current_index = match self.edit_index {
                    None => text.len() + 1,
                    Some(idx) => idx,
                };

                if current_index <= 1 && m == &-1 {
                    self.edit_index = Some(1);
                } else if current_index >= text.len() && m == &1 {
                    self.edit_index = None;
                } else {
                    self.edit_index = Some((current_index as i32 + m) as usize);
                }

                true
            }
            TmemoStateAction::RawKey(c, modifiers) => {
                return self.handle_edit_key(c.clone(), modifiers.clone());
            }
            TmemoStateAction::RawBackspace => {
                let str: &mut String;
                if self.edit_mode == EditMode::EditFront {
                    str = &mut self.current_card.as_mut().unwrap().content.front;
                } else {
                    str = &mut self.current_card.as_mut().unwrap().content.back;
                }
                if !str.is_empty() {
                    let index = match self.edit_index {
                        None => str.len() + 1,
                        Some(idx) => idx,
                    };
                    if index > 1 {
                        let mut new_str: String;
                        let part1: String = str.chars().take(index - 2).collect();
                        let part2: String = str.chars().skip(index - 1).collect();
                        new_str = part1;
                        new_str.push_str(&part2);
                        *str = new_str;
                        self.edit_index = Some(index - 1);
                        return true;
                    } else {
                        return false;
                    }
                }
                false
            }
            TmemoStateAction::ToggleClozeType => {
                let back = &mut self.current_card.as_mut().unwrap().content.back;
                if back.find("{{{").is_some() {
                    *back = back.replace("{{{", "(((");
                    *back = back.replace("}}}", ")))");
                } else if back.find("(((").is_some() {
                    *back = back.replace("(((", "{{{");
                    *back = back.replace(")))", "}}}");
                }

                true
            }
            _ => panic!("Unexpected state transition in edit mode!"),
        }
    }

    fn process_review(self: &mut TMemoInternalState, action: &TmemoStateAction) -> bool {
        match action {
            TmemoStateAction::StartEdit(mode) => {
                if self.current_card.is_none() {
                    self.edit_mode = EditMode::None;
                } else {
                    self.view = TMemoStateView::Edit;
                    self.edit_return_view = TMemoStateView::Review;
                    self.edit_mode = mode.clone();
                    self.edit_index = None;
                }
                true
            }
            TmemoStateAction::StartBaseEdit(mode) => {
                if self.current_card.is_none() {
                    self.edit_mode = EditMode::None;
                } else {
                    self.view = TMemoStateView::Edit;
                    self.edit_return_view = TMemoStateView::Review;
                    self.edit_mode = mode.clone();
                    let parent_index = self.current_card.as_ref().unwrap().content.base;
                    let edit_card = self.deck.base_cards[parent_index].clone();
                    self.current_card = Some(edit_card.into());
                    self.edit_index = None;
                }
                true
            }
            TmemoStateAction::ExitReview => {
                self.view = TMemoStateView::Main;
                self.deck.stop_review();
                true
            }
            TmemoStateAction::ShowBack => {
                self.review_show_back = true;
                true
            }
            TmemoStateAction::CardResponse(answer) => {
                self.deck.review_card(answer.clone(), &mut self.rng);
                self.set_review_card();
                self.review_show_back = false;
                true
            }
            _ => false,
        }
    }

    fn update_search_index(&mut self) {
        if self.find_state.search_results.is_empty() {
            self.find_state.search_index = 0;
        } else {
            self.find_state.search_index = self
                .find_state
                .search_index
                .min(self.find_state.search_results.len() - 1);
        }
    }

    fn update_search_results(&mut self) {
        self.find_state.search_results = self.deck.find_cards(self.find_state.search_input.clone());
        self.update_search_index();
    }

    fn process_find(self: &mut TMemoInternalState, action: &TmemoStateAction) -> bool {
        match &action {
            TmemoStateAction::RawKey(c, KeyModifiers::NONE) => {
                self.find_state.search_input.push(c.clone());
                self.update_search_results();
                return true;
            }
            TmemoStateAction::RawKey(c, KeyModifiers::SHIFT) => {
                let uppercased = c.to_uppercase().to_string();
                self.find_state.search_input.push_str(&uppercased);
                self.update_search_results();
                return true;
            }
            TmemoStateAction::RawBackspace => {
                self.find_state.search_input.pop();
                self.update_search_results();
                return true;
            }
            TmemoStateAction::Up => {
                if self.find_state.search_index > 0 {
                    self.find_state.search_index -= 1;
                }
                return true;
            }
            TmemoStateAction::Down => {
                self.find_state.search_index += 1;
                self.update_search_index();
                return true;
            }
            TmemoStateAction::StartFindEdit => {
                let card_index = self.find_state.search_results[self.find_state.search_index];
                if card_index >= self.deck.cards.len() {
                    return false;
                }
                let mut card: Card = self.deck.cards[card_index].clone();
                self.edit_index = None;
                if card.content.editable {
                    let parent_index = card.content.base;
                    card = self.deck.base_cards[parent_index].clone().into();
                } else {
                    return false;
                }
                self.edit_mode = EditMode::EditFront;
                self.view = TMemoStateView::Edit;
                self.current_card = Some(card);
                self.edit_return_view = TMemoStateView::Find;
            }
            _ => (),
        }

        return false;
    }

    pub fn process(self: &mut TMemoInternalState, action: &TmemoStateAction) -> bool {
        match &action {
            TmemoStateAction::Quit => {
                self.wants_to_quit = true;
                return false;
            }
            TmemoStateAction::Seed(seed) => {
                self.rng = SplitMix64::from_seed(seed.clone());
                return true;
            }
            TmemoStateAction::EnterView(view) => {
                if *view == TMemoStateView::Find {
                    self.find_state = FindViewState::new();
                    self.update_search_results();
                }

                self.view = view.clone();
                return true;
            }
            _ => (),
        }

        match self.view {
            TMemoStateView::Main => self.process_main_view(action),
            TMemoStateView::Review => self.process_review(action),
            TMemoStateView::Hotkeys => false,
            TMemoStateView::Find => self.process_find(action),
            TMemoStateView::Edit => self.process_edit(action),
        }
    }
}

fn to_main_action(event: KeyEvent, _state: &ApplicationState) -> Option<TmemoStateAction> {
    match (event.code, event.modifiers) {
        (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, KeyModifiers::NONE) => {
            Some(TmemoStateAction::Down)
        }
        (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, KeyModifiers::NONE) => {
            Some(TmemoStateAction::Up)
        }
        (KeyCode::Esc, KeyModifiers::NONE) => Some(TmemoStateAction::Quit),
        (KeyCode::Enter, KeyModifiers::NONE) => {
            if _state.current_state.main_index == 0 {
                Some(TmemoStateAction::StartReview)
            } else if _state.current_state.main_index == 1 {
                Some(TmemoStateAction::StartAllReview)
            } else if _state.current_state.main_index == 2 {
                Some(TmemoStateAction::EnterView(TMemoStateView::Find))
            } else if _state.current_state.main_index == 3 {
                Some(TmemoStateAction::StartHotkeys)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn to_hotkeys_action(event: KeyEvent, _state: &ApplicationState) -> Option<TmemoStateAction> {
    match (event.code, event.modifiers) {
        (KeyCode::Esc, KeyModifiers::NONE) | (KeyCode::Enter, KeyModifiers::NONE) => {
            Some(TmemoStateAction::EnterView(TMemoStateView::Main))
        }
        _ => None,
    }
}

fn to_find_action(event: KeyEvent, _state: &ApplicationState) -> Option<TmemoStateAction> {
    match (event.code, event.modifiers) {
        (KeyCode::Esc, KeyModifiers::NONE) => {
            Some(TmemoStateAction::EnterView(TMemoStateView::Main))
        }
        (KeyCode::Char('j'), KeyModifiers::CONTROL) | (KeyCode::Down, _) => {
            Some(TmemoStateAction::Down)
        }
        (KeyCode::Char('k'), KeyModifiers::CONTROL) | (KeyCode::Up, _) => {
            Some(TmemoStateAction::Up)
        }
        (KeyCode::Enter, _) => Some(TmemoStateAction::StartFindEdit),
        (KeyCode::Char(c), modifiers) => Some(TmemoStateAction::RawKey(c, modifiers)),
        (KeyCode::Backspace, _) => Some(TmemoStateAction::RawBackspace),
        _ => None,
    }
}

fn to_review_action(event: KeyEvent, state: &ApplicationState) -> Option<TmemoStateAction> {
    if state.current_state.edit_mode != EditMode::None {
        return to_edit_action(event, state);
    }

    // Answers are only valid if the back of the card is shown
    if state.current_state.review_show_back {
        match (event.code, event.modifiers) {
            (KeyCode::Char('1'), KeyModifiers::NONE) => {
                return Some(TmemoStateAction::CardResponse(ReviewAnswer::Again))
            }
            (KeyCode::Char('2'), KeyModifiers::NONE) => {
                return Some(TmemoStateAction::CardResponse(ReviewAnswer::Hard))
            }
            (KeyCode::Char('3'), KeyModifiers::NONE) => {
                return Some(TmemoStateAction::CardResponse(ReviewAnswer::Good))
            }
            (KeyCode::Char('4'), KeyModifiers::NONE) => {
                return Some(TmemoStateAction::CardResponse(ReviewAnswer::Easy))
            }
            _ => (),
        }
    }

    match (event.code, event.modifiers) {
        (KeyCode::Char('e'), KeyModifiers::CONTROL) | (KeyCode::F(12), KeyModifiers::NONE) => {
            match state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .get_editability()
            {
                Editable::Editable => Some(TmemoStateAction::StartEdit(EditMode::EditFront)),
                Editable::BaseEditable => {
                    Some(TmemoStateAction::StartBaseEdit(EditMode::EditFront))
                }
                Editable::NotEditable => None,
            }
        }
        (KeyCode::Esc, KeyModifiers::NONE) => Some(TmemoStateAction::ExitReview),
        (KeyCode::Char('b'), KeyModifiers::NONE) => {
            if state.current_state.deck.review_index.is_some() {
                Some(TmemoStateAction::CardResponse(ReviewAnswer::Bury))
            } else {
                None
            }
        }
        (KeyCode::Enter, KeyModifiers::NONE) => {
            if state.current_state.deck.review_index.is_some()
                && !state.current_state.review_show_back
            {
                Some(TmemoStateAction::ShowBack)
            } else if state.current_state.deck.review_index.is_none() {
                Some(TmemoStateAction::ExitReview)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn to_edit_action(event: KeyEvent, _state: &ApplicationState) -> Option<TmemoStateAction> {
    if event.kind == KeyEventKind::Release {
        return None;
    }

    match (event.code, event.modifiers) {
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Some(TmemoStateAction::FinishEdit(true)),
        (KeyCode::Char('t'), KeyModifiers::CONTROL) => Some(TmemoStateAction::ToggleClozeType),
        (KeyCode::Esc, _) => Some(TmemoStateAction::FinishEdit(false)),
        (KeyCode::Char(c), modifiers) => Some(TmemoStateAction::RawKey(c, modifiers)),
        (KeyCode::Backspace, _) => Some(TmemoStateAction::RawBackspace),
        (KeyCode::Enter, modifiers) => Some(TmemoStateAction::RawKey('\n', modifiers)),
        (KeyCode::Down, _) => Some(TmemoStateAction::StartEdit(EditMode::EditBack)),
        (KeyCode::Up, _) => Some(TmemoStateAction::StartEdit(EditMode::EditFront)),
        (KeyCode::Left, _) => Some(TmemoStateAction::CursorMove(-1)),
        (KeyCode::Right, _) => Some(TmemoStateAction::CursorMove(1)),
        _ => None,
    }
}

pub fn to_action(
    event: crossterm::event::Event,
    state: &ApplicationState,
) -> Option<TmemoStateAction> {
    match event {
        crossterm::event::Event::Key(key) => to_key_action(key, state),
        crossterm::event::Event::Mouse(mouse) => to_mouse_action(mouse, state),
        _ => None,
    }
}

pub fn to_mouse_action(event: MouseEvent, _state: &ApplicationState) -> Option<TmemoStateAction> {
    match event.kind {
        MouseEventKind::ScrollDown => Some(TmemoStateAction::Down),
        MouseEventKind::ScrollUp => Some(TmemoStateAction::Up),
        _ => None,
    }
}

pub fn to_key_action(event: KeyEvent, state: &ApplicationState) -> Option<TmemoStateAction> {
    if event.kind == KeyEventKind::Release {
        return None;
    }

    // Undo/redo and Ctrl+c should work in every view
    match (event.code, event.modifiers) {
        (KeyCode::Char('y'), KeyModifiers::CONTROL) => return Some(TmemoStateAction::Redo),
        (KeyCode::Char('z'), KeyModifiers::CONTROL) => return Some(TmemoStateAction::Undo),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => return Some(TmemoStateAction::Quit),
        (KeyCode::Char('o'), KeyModifiers::CONTROL) => {
            return Some(TmemoStateAction::DumpApplicationState)
        }
        _ => (),
    }

    match state.current_state.view {
        TMemoStateView::Main => to_main_action(event, state),
        TMemoStateView::Review => to_review_action(event, state),
        TMemoStateView::Hotkeys => to_hotkeys_action(event, state),
        TMemoStateView::Find => to_find_action(event, state),
        TMemoStateView::Edit => to_edit_action(event, state),
    }
}

#[cfg(test)]
mod tests {
    use super::{to_key_action, EditMode};
    use crate::card::{Card, CardCollection, CardContent};
    use crate::date::Date;
    use crate::fsrs::{FSRSState, ReviewAnswer};
    use crate::state::{ApplicationState, TMemoStateView, TmemoStateAction};
    use core::panic;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

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

    fn new_card_with_back(front: &str, back: &str) -> Card {
        Card {
            content: CardContent {
                prefix: String::new(),
                front: front.to_string(),
                back: back.to_string(),
                editable: true,
                base: 0,
                child_index: 0,
            },
            fsrs_state: FSRSState::new(default_date()),
        }
    }

    #[test]
    fn input_works() {
        let app_state = ApplicationState::new();
        let event = KeyEvent {
            code: KeyCode::Char('z'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let action = to_key_action(event, &app_state);
        let event2 = KeyEvent {
            code: KeyCode::Char('z'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let action2 = to_key_action(event2, &app_state);
        match action.unwrap() {
            TmemoStateAction::Undo => (),
            _ => panic!("expected undo"),
        }
        assert!(action2.is_none());
    }

    #[test]
    fn entering_review_works() {
        let mut app_state = ApplicationState::new();
        let event = KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let action = to_key_action(event, &app_state);
        app_state.process(action.unwrap());
        assert_eq!(app_state.current_state.view, TMemoStateView::Review);
    }

    #[test]
    fn moving_works() {
        let mut state = ApplicationState::new();
        state.process(TmemoStateAction::Down);
        assert_eq!(state.current_state.main_index, 1);
        state.process(TmemoStateAction::Down);
        assert_eq!(state.current_state.main_index, 2);
        state.process(TmemoStateAction::Up);
        assert_eq!(state.current_state.main_index, 1);
        state.process(TmemoStateAction::Up);
        assert_eq!(state.current_state.main_index, 0);
    }

    #[test]
    fn edit_works() {
        let mut state = ApplicationState::new();

        let cards = vec![
            new_card("front1"),
            new_card("front2"),
            new_card("front3"),
            new_card("front4"),
            new_card("front5"),
            new_card("front6"),
        ];
        let collection = CardCollection::from(cards).unwrap();

        state.process(TmemoStateAction::ReplaceCards(collection));
        state.process(TmemoStateAction::StartReview);
        state.process(TmemoStateAction::StartEdit(super::EditMode::EditFront));
        state.process(TmemoStateAction::CursorMove(-1));
        state.process(TmemoStateAction::RawBackspace);
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .front,
            "fron2"
        );
        state.process(TmemoStateAction::RawBackspace);
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .front,
            "fro2"
        );
        state.process(TmemoStateAction::RawKey('n', KeyModifiers::NONE));
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .front,
            "fron2"
        );
        state.process(TmemoStateAction::RawKey('t', KeyModifiers::NONE));
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .front,
            "front2"
        );
    }

    #[test]
    fn edit_works2() {
        let mut state = ApplicationState::new();

        let cards = vec![new_card("front1")];
        let collection = CardCollection::from(cards).unwrap();

        state.process(TmemoStateAction::ReplaceCards(collection));
        state.process(TmemoStateAction::StartReview);
        state.process(TmemoStateAction::StartEdit(super::EditMode::EditFront));
        state.process(TmemoStateAction::CursorMove(-1));
        for _i in 0..5 {
            state.process(TmemoStateAction::RawBackspace);
        }
        state.process(TmemoStateAction::RawBackspace);
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .front,
            "1"
        );
        state.process(TmemoStateAction::RawKey('a', KeyModifiers::NONE));
        state.process(TmemoStateAction::RawKey('b', KeyModifiers::NONE));
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .front,
            "ab1"
        );
    }

    #[test]
    fn edit_works3() {
        let mut state = ApplicationState::new();

        let cards = vec![
            new_card_with_back("front1", "{{{back}}}"),
            new_card_with_back("front2", "{{{back}}}"),
        ];
        let collection = CardCollection::from(cards).unwrap();

        state.process(TmemoStateAction::ReplaceCards(collection));
        state.process(TmemoStateAction::StartReview);
        state.process(TmemoStateAction::StartBaseEdit(super::EditMode::EditFront));
        state.process(TmemoStateAction::CursorMove(-1));
        for _i in 0..5 {
            state.process(TmemoStateAction::RawBackspace);
        }
        state.process(TmemoStateAction::RawBackspace);
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .front,
            "2"
        );
        state.process(TmemoStateAction::RawKey('a', KeyModifiers::NONE));
        state.process(TmemoStateAction::RawKey('b', KeyModifiers::NONE));
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .front,
            "ab2"
        );
        state.process(TmemoStateAction::FinishEdit(true));
        assert_eq!(state.current_state.deck.cards.len(), 2);
        assert_eq!(state.current_state.deck.base_cards.len(), 2);
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .base,
            1
        );
    }

    #[test]
    fn edit_works4() {
        let mut state = ApplicationState::new();

        let cards = vec![
            new_card_with_back("front1", "{{{back}}}"),
            new_card_with_back("front2", "back"),
        ];
        let collection = CardCollection::from(cards).unwrap();

        state.process(TmemoStateAction::ReplaceCards(collection));
        assert_eq!(state.current_state.deck.cards.len(), 2);
        assert_eq!(state.current_state.deck.base_cards.len(), 2);
        state.process(TmemoStateAction::StartReview);
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .base,
            1
        );
        state.process(TmemoStateAction::StartEdit(super::EditMode::EditFront));
        state.process(TmemoStateAction::StartEdit(super::EditMode::EditBack));
        state.process(TmemoStateAction::RawKey('}', KeyModifiers::NONE));
        state.process(TmemoStateAction::RawKey('}', KeyModifiers::NONE));
        state.process(TmemoStateAction::RawKey('}', KeyModifiers::NONE));
        state.process(TmemoStateAction::CursorMove(-7));
        state.process(TmemoStateAction::RawKey('{', KeyModifiers::NONE));
        state.process(TmemoStateAction::RawKey('{', KeyModifiers::NONE));
        state.process(TmemoStateAction::RawKey('{', KeyModifiers::NONE));
        state.process(TmemoStateAction::FinishEdit(true));

        assert_eq!(state.current_state.deck.cards.len(), 2);
        assert_eq!(state.current_state.deck.base_cards.len(), 2);
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .base,
            1
        );
        assert_eq!(
            state
                .current_state
                .current_card
                .as_ref()
                .unwrap()
                .content
                .front,
            String::from("front2\n\n{...}")
        );
    }

    #[test]
    fn edit_works5() {
        let mut state = ApplicationState::new();

        let cards = vec![new_card_with_back("front1", "{{{cloze1}}} {{{cloze2}}}")];
        let collection = CardCollection::from(cards).unwrap();

        state.process(TmemoStateAction::ReplaceCards(collection));
        state.process(TmemoStateAction::StartReview);
        state.process(TmemoStateAction::ShowBack);
        state.process(TmemoStateAction::CardResponse(ReviewAnswer::Easy));
        state.process(TmemoStateAction::StartBaseEdit(EditMode::EditFront));
        state.process(TmemoStateAction::FinishEdit(true));
        let current_card = state.current_state.current_card.clone().unwrap();
        assert_eq!(current_card.content.back, "cloze2");
    }

    #[test]
    fn review_works() {
        let mut state = ApplicationState::new();

        let cards = vec![
            new_card("front1"),
            new_card("front2"),
            new_card("front3"),
            new_card("front4"),
            new_card("front5"),
            new_card("front6"),
        ];
        let collection = CardCollection::from(cards).unwrap();

        state.process(TmemoStateAction::ReplaceCards(collection));
        state.process(TmemoStateAction::StartReview);
        assert_eq!(state.current_state.deck.active_review_count(), 6);
        assert!(state.current_state.deck.review_index.is_some());
        state.process(TmemoStateAction::ShowBack);
        assert!(state.current_state.review_show_back);
        state.process(TmemoStateAction::CardResponse(ReviewAnswer::Good));
        assert_eq!(state.current_state.deck.active_review_count(), 5);
        assert_eq!(state.current_state.review_show_back, false);
        state.process(TmemoStateAction::CardResponse(ReviewAnswer::Good));
        state.process(TmemoStateAction::CardResponse(ReviewAnswer::Good));
        state.process(TmemoStateAction::CardResponse(ReviewAnswer::Good));
        state.process(TmemoStateAction::CardResponse(ReviewAnswer::Good));
        state.process(TmemoStateAction::CardResponse(ReviewAnswer::Good));
        assert_eq!(state.current_state.deck.active_review_count(), 0);
        assert_eq!(state.current_state.deck.review_index, None);
        state.process(TmemoStateAction::StartReview);
        assert_eq!(state.current_state.deck.active_review_count(), 0);
    }

    #[test]
    fn seed_undoworks() {
        let mut state = ApplicationState::new();
        state.process(TmemoStateAction::Seed(1));
        assert_eq!(state.undo_history.len(), 1);
    }

    #[test]
    fn undo_redoworks() {
        let mut state = ApplicationState::new();
        state.process(TmemoStateAction::Down);
        assert_eq!(state.current_state.main_index, 1);
        state.process(TmemoStateAction::Up);
        assert_eq!(state.current_state.main_index, 0);
        state.process(TmemoStateAction::Undo);
        assert_eq!(state.current_state.main_index, 1);
        state.process(TmemoStateAction::Redo);
        assert_eq!(state.current_state.main_index, 0);
        state.process(TmemoStateAction::Undo);
        state.process(TmemoStateAction::Undo);
        assert_eq!(state.current_state.main_index, 0);
        state.process(TmemoStateAction::Redo);
        assert_eq!(state.current_state.main_index, 1);
        state.process(TmemoStateAction::Redo);
        assert_eq!(state.current_state.main_index, 0);
    }
}

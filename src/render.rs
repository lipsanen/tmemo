use crate::card::{Card, CardContent};
use crate::parsing::ClozeIterator;
use crate::state::EditMode;
use crate::{date::Date, state};
use ratatui::{prelude::*, widgets::*};

static ESCAPED_CHARS: &'static [char] = &[
    '\\', '*', '_', '-', '`', '{', '}', '[', ']', '(', ')', '#', '+', '.', '!', '|', '<', '>', 'x',
    '/', '%', '$',
];

pub fn format_md_text(text: &str) -> String {
    let mut iterator = text.trim().chars();
    let mut output = String::new();
    output.reserve(text.len());

    loop {
        let c = match iterator.next() {
            Some(value) => value,
            _ => break,
        };

        if c == '\t' {
            output.push_str("    ");
            continue;
        } else if c != '\\' {
            output.push(c);
            continue;
        }

        let extra = match iterator.next() {
            Some(value) => value,
            _ => {
                output.push('\\');
                break;
            }
        };

        if !ESCAPED_CHARS.contains(&extra) {
            output.push('\\');
        }
        output.push(extra);
    }

    output
}

pub fn render_app(frame: &mut Frame, state: &state::TMemoInternalState) {
    match state.view {
        state::TMemoStateView::Main => render_main(frame, state),
        state::TMemoStateView::Review => render_review(frame, state),
        state::TMemoStateView::Find => render_find(frame, state),
        state::TMemoStateView::Hotkeys => render_hotkeys(frame, state),
        state::TMemoStateView::Edit => render_edit_card(frame, state),
    }
}

fn render_output(frame: &mut Frame, state: &state::TMemoInternalState, rect: Rect) {
    let par = Paragraph::new(state.output_text.clone());
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("output");
    frame.render_widget(par.block(block), rect);
}

fn render_review_finished(frame: &mut Frame, _state: &state::TMemoInternalState) {
    let areas = Layout::new(Direction::Vertical, [Constraint::Min(1)]).split(frame.size());

    let text = Line::from(Span::raw(
        "Review has been finished! Press Enter or Esc to quit review",
    ));

    let block1 = Block::new()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded);
    let front_paragraph = Paragraph::new(text)
        .block(block1)
        .wrap(Wrap { trim: false });

    frame.render_widget(front_paragraph, areas[0]);
}

fn get_front_text(content: &CardContent) -> Vec<Line<'_>> {
    let front_text = format_md_text(&content.front);
    let mut output: Vec<Line<'_>> = Vec::new();
    if !front_text.contains("{...}") {
        let front_lines = front_text.lines();
        output.extend(front_lines.map(|x| Line::from(Span::raw(x.to_owned()))));
    } else {
        for line in front_text.lines() {
            let item = line.find("{...}");

            if item.is_none() {
                output.push(Line::from(Span::raw(line.to_owned())));
            } else {
                let index = item.unwrap();
                let mut spans = Vec::new();
                spans.push(Span::raw(line[..index].to_owned()));
                spans.push(Span::styled("{...}", Style::default().fg(Color::Green)));
                spans.push(Span::raw(line[index + 5..].to_owned()));
                output.push(Line::from(spans));
            }
        }
    }
    output
}

fn get_back_text(content: &CardContent) -> Vec<Line<'_>> {
    let back_text = format_md_text(&content.back);
    let mut output: Vec<Line<'_>> = Vec::new();
    if ClozeIterator::new(crate::parsing::ClozeType::TripleBrace, &back_text)
        .next()
        .is_none()
    {
        let back_lines = back_text.lines();
        output.extend(back_lines.map(|x| Line::from(Span::raw(x.to_owned()))));
    } else {
        for line in back_text.lines() {
            let mut iterator = ClozeIterator::new(crate::parsing::ClozeType::TripleBrace, &line);
            let item = iterator.next();

            if item.is_none() {
                output.push(Line::from(Span::raw(line.to_owned())));
            } else {
                let cloze_item = item.unwrap();
                let spans = vec![
                    Span::raw(line[..cloze_item.cloze_start].to_owned()),
                    Span::styled(
                        line[cloze_item.cloze_start + 3..cloze_item.cloze_end - 3].to_owned(),
                        Style::default().fg(Color::Green),
                    ),
                    Span::raw(line[cloze_item.cloze_end..].to_owned()),
                ];
                output.push(Line::from(spans));
            }
        }
    }
    output
}

fn render_review_in_progress(frame: &mut Frame, state: &state::TMemoInternalState, card: &Card) {
    let areas = Layout::new(
        Direction::Vertical,
        [
            Constraint::Percentage(50),
            Constraint::Max(2),
            Constraint::Min(3),
            Constraint::Max(2),
        ],
    )
    .split(frame.size());

    let text1 = get_front_text(&card.content);
    let keys = Line::from(Span::raw("[Enter] Show [Esc] Quit review [B] Bury"));

    let hard_interval = card.fsrs_state.next_interval(
        crate::fsrs::ReviewAnswer::Hard,
        &state.deck.review_date.unwrap(),
        &state.rng,
        &state.deck.params,
    );
    let good_interval = card.fsrs_state.next_interval(
        crate::fsrs::ReviewAnswer::Good,
        &state.deck.review_date.unwrap(),
        &state.rng,
        &state.deck.params,
    );
    let easy_interval = card.fsrs_state.next_interval(
        crate::fsrs::ReviewAnswer::Easy,
        &state.deck.review_date.unwrap(),
        &state.rng,
        &state.deck.params,
    );
    let answer_keys: Line;
    let text3: Vec<Line>;

    if state.review_show_back {
        text3 = get_back_text(&card.content);
        answer_keys = Line::from(Span::raw(format!(
            "[1] Again [2] Hard - {} days [3] Good - {} days [4] Easy - {} days",
            hard_interval, good_interval, easy_interval
        )));
    } else {
        text3 = vec![Line::from(Span::raw(""))];
        answer_keys = Line::from(Span::raw(""));
    }

    let block1 = Block::new()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .border_type(BorderType::Rounded)
        .title(format!(
            "Front ({}) - {} ",
            state.deck.active_review_count(),
            card.content.prefix
        ));
    let hotkeys_block = Block::new()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded);
    let answer_block = Block::new()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .border_type(BorderType::Rounded)
        .title("Back");
    let answers_hotkeys = Block::new()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded);
    let prefix_paragraph = Paragraph::new(text1)
        .block(block1)
        .wrap(Wrap { trim: false });
    let hotkey_paragraph = Paragraph::new(keys)
        .block(hotkeys_block)
        .wrap(Wrap { trim: false });
    let back_paragraph = Paragraph::new(text3)
        .block(answer_block)
        .wrap(Wrap { trim: false });
    let ahotkey_paragraph = Paragraph::new(answer_keys)
        .block(answers_hotkeys)
        .wrap(Wrap { trim: false });

    frame.render_widget(prefix_paragraph, areas[0]);
    frame.render_widget(hotkey_paragraph, areas[1]);
    frame.render_widget(back_paragraph, areas[2]);
    frame.render_widget(ahotkey_paragraph, areas[3]);
}

fn get_text_to_render(text: String, cursor_position: Option<usize>) -> String {
    match cursor_position {
        None => {
            let mut output = text.to_owned();
            output.push('█');
            output
        }
        Some(index) => {
            let after: String;
            let mut inbetween = ' ';
            let mut output: String;
            if index == 0 {
                output = String::new();
                after = text.chars().skip(1).collect();
            } else {
                let iterator = text.chars().take(index - 1);
                output = iterator.collect();
                inbetween = text.chars().nth(index - 1).unwrap_or(' ');
                after = text.chars().skip(index).collect();
            };
            output.push('█');
            if inbetween == '\n' {
                output.push('\n');
            }
            output.push_str(&after);
            output
        }
    }
}

fn render_edit_card(frame: &mut Frame, state: &state::TMemoInternalState) {
    let card = &state.current_card.as_ref().unwrap();
    let areas = Layout::new(
        Direction::Vertical,
        [
            Constraint::Percentage(50),
            Constraint::Max(2),
            Constraint::Min(3),
        ],
    )
    .split(frame.size());

    let mut front_text = card.content.front.to_string();
    let mut back_text = card.content.back.to_string();

    match state.edit_mode {
        EditMode::EditFront => {
            front_text = get_text_to_render(front_text, state.edit_index.clone())
        }
        EditMode::EditBack => back_text = get_text_to_render(back_text, state.edit_index.clone()),
        _ => panic!("not in edit mode"),
    }

    let front_lines = front_text.lines();
    let text1: Vec<Line> = front_lines
        .map(|x| Line::from(Span::raw(x.to_owned())))
        .collect();
    let keys = Line::from(Span::raw("[C-s] Save changes [Esc] Discard changes"));

    let back_lines = back_text.lines();
    let text3: Vec<Line> = back_lines
        .map(|x| Line::from(Span::raw(x.to_owned())))
        .collect();

    let block1 = Block::new()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .border_type(BorderType::Rounded)
        .title(format!("Editing card - {} ", card.content.prefix));
    let hotkeys_block = Block::new()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded);
    let block2 = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("Back");
    let prefix_paragraph = Paragraph::new(text1)
        .block(block1)
        .wrap(Wrap { trim: false });
    let hotkey_paragraph = Paragraph::new(keys)
        .block(hotkeys_block)
        .wrap(Wrap { trim: false });
    let back_paragraph = Paragraph::new(text3)
        .block(block2)
        .wrap(Wrap { trim: false });

    frame.render_widget(prefix_paragraph, areas[0]);
    frame.render_widget(hotkey_paragraph, areas[1]);
    frame.render_widget(back_paragraph, areas[2]);
}

fn render_review(frame: &mut Frame, state: &state::TMemoInternalState) {
    match &state.current_card {
        Some(card) => render_review_in_progress(frame, state, card),
        None => render_review_finished(frame, state),
    }
}

fn render_main(frame: &mut Frame, state: &state::TMemoInternalState) {
    let areas = Layout::new(
        Direction::Vertical,
        [Constraint::Percentage(66), Constraint::Percentage(34)],
    )
    .split(frame.size());

    let rows = vec![
        format!(
            "Review ({} cards)",
            state.deck.cards_to_review_count(Date::now())
        ),
        "Review all cards".to_owned(),
        "Explore cards".to_owned(),
        "Hotkeys".to_owned(),
    ];

    let text: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, x)| {
            if i as u32 == state.main_index {
                Line::from(Span::raw(format!(">{}", x)))
            } else {
                Line::from(Span::raw(format!(" {}", x)))
            }
        })
        .collect();

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("tmemo");
    frame.render_widget(Paragraph::new(text).block(block), areas[0]);
    render_output(frame, state, areas[1]);
}

fn render_find(frame: &mut Frame, state: &state::TMemoInternalState) {
    let areas = Layout::new(
        Direction::Vertical,
        [Constraint::Max(3), Constraint::Min(1)],
    )
    .split(frame.size());

    let mut row_count = areas[1].rows().count();

    if row_count >= 3 {
        row_count -= 2;
    } else {
        row_count = 1;
    }

    let result_count = state.find_state.search_results.len();
    let min_index: usize;
    let max_index: usize;
    let row_margin_after: usize = 4;

    if row_count <= 1 || result_count == 0 {
        min_index = 0;
        max_index = result_count;
    } else {
        let rows_left = state.find_state.search_results.len() - state.find_state.search_index;
        let row_after_wanted = (row_count - 1).min(row_margin_after).min(rows_left);

        if row_count - row_after_wanted <= state.find_state.search_index {
            min_index = state.find_state.search_index + row_after_wanted - row_count;
            max_index = min_index + row_count;
        } else {
            min_index = 0;
            max_index = row_count;
        }
    }

    let rows: Vec<String> = state
        .find_state
        .search_results
        .iter()
        .enumerate()
        .filter(|(index, _card_index)| *index >= min_index && *index < max_index)
        .map(|(index, card_index)| {
            let character: char;
            if index == state.find_state.search_index {
                character = '>';
            } else {
                character = ' ';
            }
            format!(
                "{} {}",
                character,
                state.deck.cards[*card_index].content.get_singleline_front()
            )
        })
        .collect();

    let text: Vec<Line> = rows
        .iter()
        .map(|x| Line::from(Span::raw(x.to_owned())))
        .collect();

    let search_block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("Search");
    frame.render_widget(
        Paragraph::new(state.find_state.search_input.clone()).block(search_block),
        areas[0],
    );

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("Results");
    frame.render_widget(Paragraph::new(text).block(block), areas[1]);
}

fn render_hotkeys(frame: &mut Frame, _state: &state::TMemoInternalState) {
    let areas = Layout::new(Direction::Vertical, [Constraint::Percentage(100)]).split(frame.size());

    let rows = [
        "k/Up arrow - Up",
        "j/Down arrow - Down",
        "Ctrl+z - Undo",
        "Ctrl+y - Redo",
        "Enter/Esc - Exit this screen",
        "Ctrl+c - Quit the application (in any view)",
        "Esc - Quit the application (in main view)",
    ];

    let text: Vec<Line> = rows
        .iter()
        .map(|x| Line::from(Span::raw(x.to_owned())))
        .collect();

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("Hotkeys");
    frame.render_widget(Paragraph::new(text).block(block), areas[0]);
}

#[cfg(test)]
mod tests {
    use crate::render::format_md_text;

    #[test]
    fn md_formatting() {
        assert_eq!(format_md_text("\\["), "[".to_owned());
        assert_eq!(format_md_text("\\<"), "<".to_owned());
        assert_eq!(format_md_text("\\{"), "{".to_owned());
        assert_eq!(format_md_text("\\[test"), "[test".to_owned());
        assert_eq!(format_md_text("\\/"), "/".to_owned());
    }
}

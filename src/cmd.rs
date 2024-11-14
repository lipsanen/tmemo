use crate::date::Date;
use crate::deck::Deck;
use crate::migrations;
use crate::rand::SplitMix64;
use crate::{cardcache::CardCache, fsrs::ReviewAnswer};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Cli {
    pub command: Option<Command>,
    pub from_stdin: bool,
    pub state_from_file: Option<String>,
}

pub enum Command {
    Init,
    Print,
    PrintHeaders,
    PrintOrphans,
    DeleteOrphans,
    Update,
    Schedule(u32, u32),
    ScheduleRandom(f64),
    ExportReviewLogs,
    Accuracy,
    Find(String),
    SimulateReview(usize),
    Migrate,
}

struct ReviewData {
    pub cards: usize,
}

fn simulate_review(mut deck: Deck, days: usize) -> Vec<ReviewData> {
    let mut output = Vec::with_capacity(days);
    let current_day = Date::now();
    let mut rng = SplitMix64::from_seed(0);

    for i in 0..days {
        let review_day = current_day.checked_add_days(i as i32).unwrap();
        deck.start_review(review_day, &mut rng);
        output.push(ReviewData {
            cards: deck.review_indices.len(),
        });

        while let Some(card) = deck.get_review_card() {
            let ret = card.fsrs_state.retention(&review_day);
            let rng_result = rng.next_float(0.0, 1.0);
            let answer = if rng_result < ret {
                ReviewAnswer::Good
            } else {
                ReviewAnswer::Again
            };

            deck.review_card(answer, &mut rng);
        }
    }

    output
}

impl Cli {
    pub fn parse(mut args: std::env::Args) -> Cli {
        if args.len() <= 1 {
            return Cli {
                command: None,
                from_stdin: false,
                state_from_file: None,
            };
        }

        let random_schedule_help_text = "usage: tmemo schedule-random [fraction], e.g. 0.1 to generate reviews between 0.9 and 1.1";
        let schedule_help_text = "usage: tmemo schedule <days> [max cards per day]";

        args.next();

        let mut cli = Cli {
            command: None,
            from_stdin: false,
            state_from_file: None,
        };

        while let Some(arg) = args.next() {
            let command = match arg.as_str() {
                "init" => Some(Command::Init),
                "print" => Some(Command::Print),
                "accuracy" => Some(Command::Accuracy),
                "print-headers" => Some(Command::PrintHeaders),
                "print-orphans" => Some(Command::PrintOrphans),
                "delete-orphans" => Some(Command::DeleteOrphans),
                "update" => Some(Command::Update),
                "review-log" => Some(Command::ExportReviewLogs),
                "migrate" => Some(Command::Migrate),
                "schedule-random" => {
                    let fraction: f64 = match args.next() {
                        None => 0.1,
                        Some(frac) => frac.parse().expect(random_schedule_help_text),
                    };

                    if fraction < 0.0 || fraction > 1.0 {
                        panic!("Fraction should be between 0 and 1");
                    }

                    Some(Command::ScheduleRandom(fraction))
                }
                "schedule" => {
                    let days: u32 = args
                        .next()
                        .expect(schedule_help_text)
                        .parse()
                        .expect(schedule_help_text);
                    let max_cards: u32 = match args.next() {
                        None => 1,
                        Some(max) => max.parse().expect(schedule_help_text),
                    };
                    Some(Command::Schedule(days, max_cards))
                }
                "find" => {
                    let search_string = args.next().expect("Expected search string after find");
                    Some(Command::Find(search_string))
                }
                "simulate" => {
                    let days: usize = args
                        .next()
                        .expect("number of days expected after simulate")
                        .parse()
                        .expect("expected valid unsigned integer number of days");
                    Some(Command::SimulateReview(days))
                }
                "-s" => {
                    cli.from_stdin = true;
                    None
                }
                "-l" => {
                    cli.state_from_file = Some(args.next().expect("filepath expected after -l"));
                    None
                }
                _ => None,
            };
            if command.is_some() {
                cli.command = command;
            }
        }

        cli
    }

    pub fn run(&self) {
        let result;
        if self.from_stdin {
            let input = std::io::read_to_string(std::io::stdin()).unwrap();
            result = Deck::load_from_tsv(input);
        } else {
            result = Deck::load_from_file();
        }

        match self.command.as_ref().unwrap() {
            Command::Init => {
                if result.is_ok() {
                    panic!("A deck has already been initialized!");
                }
                let mut deck = Deck::new();
                deck.save_to_file().unwrap();
            },
            Command::Print => {
                let deck = result.unwrap();
                deck.print_card_data();
            }
            Command::PrintHeaders => {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    "Days to review",
                    "Difficulty",
                    "Stability",
                    "Prefix",
                    "Front",
                    "Back",
                    "Date added",
                    "Complete review history",
                    "Last review",
                );
            }
            Command::PrintOrphans => {
                let deck = result.unwrap();
                for card in &deck.orphans {
                    println!(
                        "{} - {}",
                        card.content.prefix,
                        card.content.get_singleline_front()
                    );
                }
            }
            Command::DeleteOrphans => {
                let mut deck = result.unwrap();
                let count = deck.orphans.len();
                deck.orphans.clear();
                deck.save_to_file().unwrap();
                println!("{} orphans deleted", count);
            }
            Command::Update => {
                let mut deck = result.unwrap();
                let mut cache = CardCache::new();
                let cards = cache.get_all_cards_in_work_directory(None).unwrap();
                deck.replace_cards(cards, Date::now()).unwrap();
                deck.save_to_file().unwrap();
                println!("Deck updated");
            }
            Command::Schedule(days, max_cards) => {
                let mut deck = result.unwrap();
                let today = Date::now();
                println!("Scheduling with days {}, max_cards {}", days, max_cards);
                deck.reschedule(today, days.clone() as i32, max_cards.clone() as usize);
                deck.save_to_file().unwrap();
            }
            Command::ScheduleRandom(frac) => {
                let mut deck = result.unwrap();
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                println!(
                    "Scheduling with rng, between {} and {} of optimal length",
                    1.0 - frac,
                    1.0 + frac
                );
                let mut generator = SplitMix64::from_seed(seed);
                deck.random_reschedule_fractional(*frac, &mut generator);
                deck.save_to_file().unwrap();
            }
            Command::ExportReviewLogs => {
                let deck = result.unwrap();
                let epoch = Date::from_yo_opt(1970, 1).unwrap();
                println!("card_id,review_time,review_rating,review_state,review_duration");
                for (card_index, card) in deck.cards.iter().enumerate() {
                    let review_log = &card.fsrs_state.review_log;
                    if card.fsrs_state.review_log.is_empty() || !card.fsrs_state.complete_history {
                        continue;
                    }
                    let mut previous_timestamp: u64 = 0;
                    for review_index in 0..review_log.len() {
                        let timestamp: u64;
                        if review_index == 0
                            || review_log[review_index - 1].day != review_log[review_index].day
                        {
                            // Set the timestamp to be in noon
                            timestamp =
                                ((review_log[review_index].day.day - epoch.day) as u64 * 24 + 12)
                                    * 60
                                    * 60
                                    * 1000;
                        } else {
                            // exact time of reviews is not kept, only day so add 10 seconds for the timestamp
                            timestamp = previous_timestamp + 10000;
                        }
                        println!(
                            "{card_index},{timestamp},{},,",
                            match review_log[review_index].answer {
                                ReviewAnswer::Again => 1,
                                ReviewAnswer::Hard => 2,
                                ReviewAnswer::Good => 3,
                                ReviewAnswer::Easy => 4,
                                ReviewAnswer::Bury =>
                                    panic!("bury is not a valid answer in review log"),
                            }
                        );
                        previous_timestamp = timestamp;
                    }
                }
            }
            Command::Accuracy => {
                let deck = result.unwrap();
                let data = deck.get_accuracy_data(Date::now());
                for datum in data {
                    let day = datum.0;
                    let answer_tuple = datum.1;
                    let correct = answer_tuple.0;
                    let total = answer_tuple.1;
                    let accuracy = correct as f64 / total as f64;
                    println!("{day}\t{accuracy}\t{correct}\t{total}");
                }
            }
            Command::Find(search_string) => {
                let deck = result.unwrap();
                let card_indices = deck.find_cards(search_string.clone());
                for index in card_indices {
                    let card = &deck.cards[index];
                    println!("{}", card.format_to_tsv(Date::now()));
                }
            }
            Command::SimulateReview(days) => {
                let deck = result.unwrap();
                let sim = simulate_review(deck, *days);
                for (index, data) in sim.into_iter().enumerate() {
                    println!("{} {}", index, data.cards);
                }
            }
            Command::Migrate => {
                migrations::migrate_deck("tmemodeck.json".into()).unwrap();
            }
        }
    }
}

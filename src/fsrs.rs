use crate::{date::Date, rand::SplitMix64};
use serde::{de::Error, Deserialize, Serialize};
use std::f64::consts::E;

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct FSRSState {
    pub date_added: Date,
    pub last_review: Date,
    pub review_date: Date,
    pub difficulty: f64,
    pub stability: f64,
    pub buried: bool,
    pub complete_history: bool,
    #[serde(default)]
    pub review_log: Vec<ReviewLogItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReviewLogItem {
    pub answer: ReviewAnswer,
    pub day: Date,
}

impl ReviewLogItem {
    pub fn from(value: i64) -> Result<Self, ()> {
        let answer_val = match value >> 32 {
            0 => ReviewAnswer::Bury,
            1 => ReviewAnswer::Again,
            2 => ReviewAnswer::Hard,
            3 => ReviewAnswer::Good,
            4 => ReviewAnswer::Easy,
            _ => return Err(()),
        };
        let day = Date {
            day: (value & 0xFFFFFFFF) as i32,
        };
        Ok(ReviewLogItem {
            day,
            answer: answer_val,
        })
    }

    pub fn encode(&self) -> i64 {
        let value: i64 = match self.answer {
            ReviewAnswer::Bury => 0,
            ReviewAnswer::Again => 1,
            ReviewAnswer::Hard => 2,
            ReviewAnswer::Good => 3,
            ReviewAnswer::Easy => 4,
        };
        (value << 32) | self.day.day as i64
    }
}

impl Serialize for ReviewLogItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value: i64 = self.encode();
        serializer.serialize_i64(value)
    }
}

struct I64Visitor;
impl serde::de::Visitor<'_> for I64Visitor {
    type Value = i64;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Only numeric strings or i64 can be deserialised to i64")
    }
    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(v)
    }
    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(v as i64)
    }
}

impl<'de> Deserialize<'de> for ReviewLogItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: i64 = deserializer.deserialize_i64(I64Visitor)?;
        Ok(ReviewLogItem::from(value).unwrap())
    }
}

#[derive(Clone, Deserialize, Debug, PartialEq, Serialize)]
pub enum ReviewAnswer {
    Again,
    Hard,
    Good,
    Easy,
    Bury,
}

#[derive(Debug, PartialEq)]
pub enum ReviewResult {
    Again,
    Discard,
}

const FACTOR: f64 = 19.0 / 81.0;
const INV_DECAY: f64 = 1.0 / DECAY;
const DECAY: f64 = -0.5;
const DEFAULT_W: [f64; 17] = [
    0.5701, 1.4436, 4.1386, 10.9355, 5.1443, 1.2006, 0.8627, 0.0362, 1.629, 0.1342, 1.0166, 2.1174,
    0.0839, 0.3204, 1.4676, 0.219, 2.8237,
];
const RANDOMNESS: f64 = 0.1; // Determines the range [1.0-RANDOMNESS, 1.0+RANDOMNESS] where the next review will land

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FSRSParams {
    pub w: [f64; 17],
    pub target_retention: f64,
}

impl Default for FSRSParams {
    fn default() -> Self {
        Self::new()
    }
}

impl FSRSParams {
    pub fn new() -> FSRSParams {
        FSRSParams {
            w: DEFAULT_W,
            target_retention: 0.9,
        }
    }
}

fn new_difficulty(d: f64, g: f64, params: &FSRSParams) -> f64 {
    let mut new_d = d - params.w[6] * (g - 3.0);
    new_d = params.w[7] * (params.w[4] - new_d) + new_d;
    new_d.clamp(1.0, 10.0)
}

fn new_stability_correct(
    difficulty: f64,
    stability: f64,
    retention: f64,
    answer: ReviewAnswer,
    params: &FSRSParams,
) -> f64 {
    let hard_penalty = if let ReviewAnswer::Hard = answer {
        params.w[15]
    } else {
        1.0
    };
    let easy_bonus = if let ReviewAnswer::Easy = answer {
        params.w[16]
    } else {
        1.0
    };

    stability
        * (E.powf(params.w[8])
            * (11.0 - difficulty)
            * stability.powf(-params.w[9])
            * (E.powf(params.w[10] * (1.0 - retention)) - 1.0)
            * hard_penalty
            * easy_bonus
            + 1.0)
}

fn new_stability_incorrect(
    difficulty: f64,
    stability: f64,
    retention: f64,
    params: &FSRSParams,
) -> f64 {
    params.w[11]
        * difficulty.powf(-params.w[12])
        * ((stability + 1.0).powf(params.w[13]) - 1.0)
        * E.powf(params.w[14] * (1.0 - retention))
}

fn grade_f64(answer: ReviewAnswer) -> f64 {
    match answer {
        ReviewAnswer::Easy => 4.0,
        ReviewAnswer::Good => 3.0,
        ReviewAnswer::Hard => 2.0,
        ReviewAnswer::Again => 1.0,
        ReviewAnswer::Bury => panic!("unexpected bury"),
    }
}

fn power_forgetting_curve(delta_t: f64, stability: f64) -> f64 {
    (1.0 + FACTOR * delta_t / stability).powf(DECAY)
}

impl FSRSState {
    pub fn new(date: Date) -> FSRSState {
        FSRSState {
            date_added: date,
            last_review: date,
            review_date: date,
            difficulty: 0.0,
            stability: 0.0,
            buried: false,
            complete_history: true,
            review_log: vec![],
        }
    }

    pub fn first_review(&self) -> bool {
        self.complete_history && self.review_log.is_empty()
    }

    pub fn retention(&self, date: &Date) -> f64 {
        let mut t: f64 = (date.day - self.last_review.day).into();
        t = if t >= 1.0 { t } else { 1.0 };
        power_forgetting_curve(t, self.stability)
    }

    pub fn interval(&self, params: &FSRSParams) -> f64 {
        (self.stability / FACTOR * (params.target_retention.powf(INV_DECAY) - 1.0)).max(1.0)
    }

    fn update_review_success(&mut self, date: &Date, fraction: f64, params: &FSRSParams) {
        let days = (self.interval(params) * fraction).round() as i32;
        self.last_review = *date;
        self.review_date = self.last_review.checked_add_days(days).unwrap();
    }

    fn update_review_failure(&mut self, date: &Date) {
        self.review_date = *date;
        self.last_review = *date;
    }

    fn handle_initial_review(
        &mut self,
        answer: ReviewAnswer,
        date: &Date,
        fraction: f64,
        params: &FSRSParams,
    ) -> ReviewResult {
        match answer {
            ReviewAnswer::Easy => {
                self.stability = params.w[3];
                self.difficulty = params.w[4] - params.w[5];
                self.update_review_success(date, fraction, params);
                ReviewResult::Discard
            }
            ReviewAnswer::Good => {
                self.stability = params.w[2];
                self.difficulty = params.w[4];
                self.update_review_success(date, fraction, params);
                ReviewResult::Discard
            }
            ReviewAnswer::Hard => {
                self.stability = params.w[1];
                self.difficulty = params.w[4] + params.w[5];
                self.update_review_success(date, fraction, params);
                ReviewResult::Discard
            }
            ReviewAnswer::Again => {
                self.stability = params.w[0];
                self.difficulty = params.w[4] + params.w[5] * 2.0;
                self.update_review_failure(date);
                ReviewResult::Again
            }
            ReviewAnswer::Bury => panic!("unexpected bury"),
        }
    }

    pub fn next_interval(
        &self,
        answer: ReviewAnswer,
        date: &Date,
        rng: &SplitMix64,
        params: &FSRSParams,
    ) -> u32 {
        let mut cloned = self.clone();
        let mut rng = rng.clone();
        cloned.review(
            answer,
            date,
            false,
            rng.next_float(1.0 - RANDOMNESS, 1.0 + RANDOMNESS),
            params,
        );
        (cloned.review_date.day - date.day) as u32
    }

    pub fn review_with_rng(
        &mut self,
        answer: ReviewAnswer,
        date: &Date,
        track_history: bool,
        rng: &mut SplitMix64,
        params: &FSRSParams,
    ) -> ReviewResult {
        self.review(
            answer,
            date,
            track_history,
            rng.next_float(1.0 - RANDOMNESS, 1.0 + RANDOMNESS),
            params,
        )
    }

    pub fn review(
        &mut self,
        answer: ReviewAnswer,
        date: &Date,
        track_history: bool,
        fraction: f64,
        params: &FSRSParams,
    ) -> ReviewResult {
        if let ReviewAnswer::Bury = answer {
            self.buried = true;
            return ReviewResult::Discard;
        }
        let first_review = self.first_review();

        if track_history {
            self.review_log.push(ReviewLogItem {
                answer: answer.clone(),
                day: *date,
            });
        } else {
            self.complete_history = false;
        }

        if first_review {
            let result = self.handle_initial_review(answer, date, fraction, params);
            return result;
        }

        if let ReviewAnswer::Again = answer {
            let retention = self.retention(date);
            self.stability =
                new_stability_incorrect(self.difficulty, self.stability, retention, params);
            self.difficulty = new_difficulty(self.difficulty, grade_f64(answer), params);
            self.update_review_failure(date);
            ReviewResult::Again
        } else {
            let retention = self.retention(date);
            self.stability = new_stability_correct(
                self.difficulty,
                self.stability,
                retention,
                answer.clone(),
                params,
            );
            self.difficulty = new_difficulty(self.difficulty, grade_f64(answer), params);
            self.update_review_success(date, fraction, params);
            ReviewResult::Discard
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FSRSParams;
    use super::ReviewLogItem;
    use crate::date::Date;
    use crate::fsrs::new_difficulty;
    use crate::fsrs::new_stability_incorrect;
    use crate::fsrs::power_forgetting_curve;
    use crate::fsrs::FSRSState;
    use crate::fsrs::ReviewAnswer;
    use crate::rand::SplitMix64;
    use std::iter::zip;

    fn default_date() -> Date {
        Date::from_yo_opt(3000, 1).unwrap()
    }

    struct ReviewItem {
        answer: ReviewAnswer,
        days: i32,
    }

    fn schedule_tester2(reviews: Vec<ReviewItem>, expected_result: i32) {
        let params = FSRSParams::new();
        let mut state = FSRSState::new(default_date());
        let mut current_date = default_date();

        for review in reviews {
            state.review(review.answer, &current_date, false, 1.0, &params);
            current_date = current_date.checked_add_days(review.days).unwrap();
        }

        let interval = state.interval(&params).round() as i32;
        assert_eq!(interval, expected_result);
    }

    fn schedule_tester(reviews: Vec<ReviewAnswer>, expected_results: Vec<i32>) {
        let params = FSRSParams::new();
        let mut state = FSRSState::new(default_date());
        let mut current_date = default_date();

        for (review, expected) in zip(reviews, expected_results) {
            state.review(review, &current_date, false, 1.0, &params);
            let result = state.review_date.day - current_date.day;
            assert_eq!(result, expected);
            current_date = state.review_date;
        }
    }

    #[test]
    fn random_tester() {
        let params = FSRSParams::new();
        let mut date = default_date();
        let mut state = FSRSState::new(date);
        let mut rng = SplitMix64::from_seed(3);
        state.review_with_rng(ReviewAnswer::Good, &date, false, &mut rng, &params);
        assert_eq!(false, state.first_review());
        date = state.review_date;
        let next = state.next_interval(ReviewAnswer::Easy, &date, &rng, &params);
        assert_eq!(next, 35);
    }

    #[test]
    fn scheduling_works() {
        schedule_tester(vec![ReviewAnswer::Again], vec![0]);
        schedule_tester(vec![ReviewAnswer::Hard], vec![1]);
        schedule_tester(vec![ReviewAnswer::Good], vec![4]);
        schedule_tester(vec![ReviewAnswer::Easy], vec![11]);
        schedule_tester(
            vec![ReviewAnswer::Easy, ReviewAnswer::Again, ReviewAnswer::Easy],
            vec![11, 0, 10],
        );
        schedule_tester(
            vec![ReviewAnswer::Good, ReviewAnswer::Again, ReviewAnswer::Easy],
            vec![4, 0, 8],
        );
    }

    #[test]
    fn scheduling_works2() {
        schedule_tester2(
            vec![
                ReviewItem {
                    answer: ReviewAnswer::Again,
                    days: 1,
                },
                ReviewItem {
                    answer: ReviewAnswer::Good,
                    days: 3,
                },
                ReviewItem {
                    answer: ReviewAnswer::Good,
                    days: 8,
                },
                ReviewItem {
                    answer: ReviewAnswer::Good,
                    days: 21,
                },
                ReviewItem {
                    answer: ReviewAnswer::Good,
                    days: 0,
                },
            ],
            48,
        );
    }

    // I've copied these test results from the FSRS-rs repo to try to ensure that the algorithm works the same
    #[test]
    fn power_forgetting_curve_works() {
        let delta_t = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let stability = [1.0, 2.0, 3.0, 4.0, 4.0, 2.0];
        let expected = [1.0, 0.946059, 0.9299294, 0.9221679, 0.9, 0.79394597];
        for i in 0..5 {
            let retention = power_forgetting_curve(delta_t[i], stability[i]);
            assert!((retention - expected[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn difficulty_works() {
        let params = FSRSParams::new();
        let difficulties = [5.0; 4];
        let answers = [1.0, 2.0, 3.0, 4.0];
        let new_difficulties = [6.6681643, 5.836694, 5.0052238, 4.1737533];
        for i in 0..4 {
            let difficulty = new_difficulty(difficulties[i], answers[i], &params);
            assert!((difficulty - new_difficulties[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn stability_works() {
        let params = FSRSParams::new();
        let stabilities = [5.0; 4];
        let difficulties = [1.0, 2.0, 3.0, 4.0];
        let retentions = [0.9, 0.8, 0.7, 0.6];
        let new_stabs_failure = [1.9016013, 2.0777826, 2.3257504, 2.6291647];
        for i in 0..4 {
            let stab_after_failure =
                new_stability_incorrect(difficulties[i], stabilities[i], retentions[i], &params);
            assert!((stab_after_failure - new_stabs_failure[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn retention_works() {
        let date = Date { day: 1000 };
        let mut state = FSRSState::new(date);
        let mut params = FSRSParams::new();
        params.target_retention = 0.95;
        state.review(ReviewAnswer::Good, &date, false, 1.0, &params);
        assert_eq!(2, state.review_date.day - date.day);
        let mut review_day = state.review_date;
        state.review(ReviewAnswer::Good, &review_day, false, 1.0, &params);
        assert_eq!(4, state.review_date.day - review_day.day);
        review_day = state.review_date;
        state.review(ReviewAnswer::Good, &review_day, false, 1.0, &params);
        assert_eq!(9, state.review_date.day - review_day.day);
        review_day = state.review_date;
        state.review(ReviewAnswer::Good, &review_day, false, 1.0, &params);
        assert_eq!(18, state.review_date.day - review_day.day);
    }

    #[test]
    fn review_log_serialization_works() {
        for i in 1000..2000 {
            for answer in [
                ReviewAnswer::Bury,
                ReviewAnswer::Again,
                ReviewAnswer::Hard,
                ReviewAnswer::Good,
                ReviewAnswer::Easy,
            ] {
                let item = ReviewLogItem {
                    day: Date { day: i },
                    answer: answer.clone(),
                };
                let encoded = item.encode();
                let decoded = ReviewLogItem::from(encoded).unwrap();
                assert_eq!(decoded, item);
            }
        }
    }
}

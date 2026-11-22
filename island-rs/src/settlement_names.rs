use std::collections::{BTreeMap, HashSet};

use crate::rng::Rng;

const MIN_NAME_LENGTH: usize = 6;
const MAX_NAME_LENGTH: usize = 26;
const MARKOV_ATTEMPTS: usize = 48;

const FALLBACK_CONSONANTS: &[u8] = b"bdfghklmnprstv";
const FALLBACK_VOWELS: &[u8] = b"aeiou";
const FALLBACK_ENDINGS: &[u8] = b"lnrs";
const FALLBACK_PAIR_COUNT: u32 = 4;
const FALLBACK_NAME_COUNT: u64 = ((FALLBACK_CONSONANTS.len() * FALLBACK_VOWELS.len()) as u64)
    .pow(FALLBACK_PAIR_COUNT)
    * FALLBACK_ENDINGS.len() as u64;

type Pair = [char; 2];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Token {
    Pair(Pair),
    Letter(char),
    Space,
    End,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum WordPosition {
    First,
    Later,
}

#[derive(Debug, Default)]
pub(crate) struct SettlementNames {
    starts: BTreeMap<Pair, u64>,
    continuations: BTreeMap<(WordPosition, Token), BTreeMap<Token, u64>>,
    source_names: HashSet<String>,
}

impl SettlementNames {
    pub(crate) fn from_corpus(corpus: &str) -> Self {
        let mut model = Self::default();
        let names = corpus
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .map(normalize)
            .filter(|name| !name.is_empty());

        for name in names {
            model.learn(&name);
            model.source_names.insert(name);
        }
        model
    }

    /// Assigns names in seed order and returns them in the caller's order.
    pub(crate) fn names(&self, seeds: impl IntoIterator<Item = u64>) -> Vec<String> {
        let mut requests: Vec<_> = seeds
            .into_iter()
            .enumerate()
            .map(|(index, seed)| (seed, index))
            .collect();
        requests.sort_unstable();

        let mut used = HashSet::new();
        let mut names = vec![String::new(); requests.len()];
        for group in requests.chunk_by(|left, right| left.0 == right.0) {
            let (seed, first_index) = group[0];
            let candidate = self.unused_name(seed, &used);
            let name = capitalize(&candidate);
            used.insert(candidate);
            for &(_, index) in &group[1..] {
                names[index].clone_from(&name);
            }
            names[first_index] = name;
        }
        names
    }

    fn learn(&mut self, name: &str) {
        let mut words = name.split(' ');
        let Some(first_word) = words.next() else {
            return;
        };
        let mut first_tokens = word_tokens(first_word);
        let Some(Token::Pair(start)) = first_tokens.next() else {
            return;
        };

        *self.starts.entry(start).or_default() += 1;
        let mut state = (WordPosition::First, Token::Pair(start));
        for next in first_tokens {
            self.learn_transition(state, next);
            state.1 = next;
        }

        for word in words {
            self.learn_transition(state, Token::Space);
            state = (WordPosition::Later, Token::Space);
            for next in word_tokens(word) {
                self.learn_transition(state, next);
                state.1 = next;
            }
        }
        self.learn_transition(state, Token::End);
    }

    fn learn_transition(&mut self, state: (WordPosition, Token), next: Token) {
        *self
            .continuations
            .entry(state)
            .or_default()
            .entry(next)
            .or_default() += 1;
    }

    fn unused_name(&self, seed: u64, used: &HashSet<String>) -> String {
        let mut rng = Rng::new(seed);
        let available = |candidate: &String| {
            !self.source_names.contains(candidate) && !used.contains(candidate)
        };

        if !self.starts.is_empty()
            && let Some(name) = (0..MARKOV_ATTEMPTS)
                .filter_map(|_| self.sample(&mut rng))
                .find(available)
        {
            return name;
        }

        let start = choose(&self.starts, &mut rng);
        let offset = rng.next_u64() % FALLBACK_NAME_COUNT;
        (0..FALLBACK_NAME_COUNT)
            .map(|step| fallback_name((offset + step) % FALLBACK_NAME_COUNT, start))
            .find(available)
            .unwrap_or_else(|| fallback_name(offset, start))
    }

    fn sample(&self, rng: &mut Rng) -> Option<String> {
        let mut word = WordPosition::First;
        let mut token = Token::Pair(choose(&self.starts, rng)?);
        let mut name = String::with_capacity(MAX_NAME_LENGTH);
        let mut length = 0;
        loop {
            match token {
                Token::Pair(pair) => {
                    name.extend(pair);
                    length += 2;
                }
                Token::Letter(letter) => {
                    name.push(letter);
                    length += 1;
                }
                Token::Space if word == WordPosition::First => {
                    name.push(' ');
                    length += 1;
                    word = WordPosition::Later;
                }
                Token::Space => return None,
                Token::End => return (length >= MIN_NAME_LENGTH).then_some(name),
            }
            if length > MAX_NAME_LENGTH {
                return None;
            }
            token = choose(self.continuations.get(&(word, token))?, rng)?;
        }
    }
}

fn normalize(line: &str) -> String {
    let mut name = String::with_capacity(line.len());
    for word in line.split_whitespace() {
        let mut letters = word
            .chars()
            .flat_map(char::to_lowercase)
            .filter(|letter| letter.is_alphabetic());
        if let Some(first) = letters.next() {
            if !name.is_empty() {
                name.push(' ');
            }
            name.push(first);
            name.extend(letters);
        }
    }
    name
}

fn word_tokens(word: &str) -> impl Iterator<Item = Token> + '_ {
    let mut letters = word.chars();
    std::iter::from_fn(move || {
        let first = letters.next()?;
        Some(match letters.next() {
            Some(second) => Token::Pair([first, second]),
            None => Token::Letter(first),
        })
    })
}

fn choose<T: Copy>(counts: &BTreeMap<T, u64>, rng: &mut Rng) -> Option<T> {
    let total: u64 = counts.values().sum();
    if total == 0 {
        return None;
    }

    let mut ticket = rng.next_u64() % total;
    for (&choice, &count) in counts {
        if ticket < count {
            return Some(choice);
        }
        ticket -= count;
    }
    None
}

fn capitalize(name: &str) -> String {
    let mut title = String::with_capacity(name.len());
    let mut start = true;
    for letter in name.chars() {
        let mut uppercase = letter.to_uppercase();
        title.push(match (start, uppercase.next(), uppercase.next()) {
            (true, Some(upper), None) if upper.to_lowercase().eq([letter]) => upper,
            _ => letter,
        });
        start = letter == ' ';
    }
    title
}

fn fallback_name(index: u64, start: Option<Pair>) -> String {
    let mut index = usize::try_from(index).expect("fallback index fits in usize");
    let mut name = String::with_capacity(2 * FALLBACK_PAIR_COUNT as usize + 3);
    if let Some(pair) = start {
        name.extend(pair);
    }
    for _ in 0..FALLBACK_PAIR_COUNT {
        name.push(char::from(
            FALLBACK_CONSONANTS[index % FALLBACK_CONSONANTS.len()],
        ));
        index /= FALLBACK_CONSONANTS.len();
        name.push(char::from(FALLBACK_VOWELS[index % FALLBACK_VOWELS.len()]));
        index /= FALLBACK_VOWELS.len();
    }
    name.push(char::from(FALLBACK_ENDINGS[index]));
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_non_overlapping_pairs_and_both_kinds_of_endings() {
        let model = SettlementNames::from_corpus("Élve-ra\nÉlvera\nOlveran\n");
        let first = |token| (WordPosition::First, token);

        assert_eq!(model.starts[&['é', 'l']], 2);
        assert_eq!(model.starts[&['o', 'l']], 1);
        assert_eq!(
            model.continuations[&first(Token::Pair(['v', 'e']))][&Token::Pair(['r', 'a'])],
            3
        );
        assert_eq!(
            model.continuations[&first(Token::Pair(['r', 'a']))][&Token::End],
            2
        );
        assert_eq!(
            model.continuations[&first(Token::Pair(['r', 'a']))][&Token::Letter('n')],
            1
        );
        assert_eq!(
            model.continuations[&first(Token::Letter('n'))][&Token::End],
            1
        );
        assert!(
            !model
                .continuations
                .contains_key(&first(Token::Pair(['l', 'v'])))
        );
    }

    #[test]
    fn learns_spaces_and_only_starts_at_the_beginning_of_a_full_name() {
        let model = SettlementNames::from_corpus(
            " Ashford  Glen \nAshford\nOakford\tGlenmere\nOakford\nGreat Oak Glen\n# Branbridge",
        );
        assert_eq!(
            model.starts,
            BTreeMap::from([(['a', 's'], 2), (['g', 'r'], 1), (['o', 'a'], 2)])
        );
        assert_eq!(
            model.continuations[&(WordPosition::First, Token::Letter('d'))][&Token::Space],
            2
        );
        assert_eq!(
            model.continuations[&(WordPosition::Later, Token::Space)][&Token::Pair(['g', 'l'])],
            3
        );

        let names = model.names(0..200);
        assert!(names.iter().any(|name| name == "Ashford Glenmere"));
        assert!(names.iter().any(|name| name == "Oakford Glen"));
        assert!(names.iter().all(|name| {
            let start = [
                name.as_bytes()[0].to_ascii_lowercase() as char,
                name.as_bytes()[1] as char,
            ];
            model.starts.contains_key(&start) && name.split(' ').count() <= 2
        }));
    }

    #[test]
    fn generates_novel_names_with_learned_odd_and_even_endings() {
        let model = SettlementNames::from_corpus("Élve-ra\nOlveran");
        let names = model.names(0..25);

        assert!(names.iter().any(|name| name == "Élveran"));
        assert!(names.iter().any(|name| name == "Olvera"));
        assert!(
            !names
                .iter()
                .any(|name| name == "Élvera" || name == "Olveran")
        );
        assert_eq!(names.iter().collect::<HashSet<_>>().len(), names.len());
    }

    #[test]
    fn repeated_seeds_keep_their_name_when_the_request_order_changes() {
        let model = SettlementNames::from_corpus("Élvera\nOlveran");
        let first = model.names([52, 87, 3, 52, 42]);
        let second = model.names([3, 42, 52, 87]);

        assert_eq!(first[0], first[3]);
        assert_eq!(first[2], second[0]);
        assert_eq!(first[4], second[1]);
        assert_eq!(first[0], second[2]);
        assert_eq!(first[1], second[3]);
        assert_eq!(first.iter().collect::<HashSet<_>>().len(), second.len());
    }

    #[test]
    fn unusable_corpora_still_return_bounded_unique_names() {
        for corpus in [
            "",
            "🏝\n# only a comment\n1234",
            "X\nAn\nXe",
            "Anranranranranranra",
        ] {
            let model = SettlementNames::from_corpus(corpus);
            let names = model.names(0..500);

            assert_eq!(model.names(0..500), names);
            assert_eq!(names.iter().collect::<HashSet<_>>().len(), names.len());
            assert!(names.iter().all(|name| {
                (MIN_NAME_LENGTH..=MAX_NAME_LENGTH).contains(&name.chars().count())
                    && name.chars().all(char::is_alphabetic)
            }));
        }
    }
}

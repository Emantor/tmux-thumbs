use std::collections::HashMap;
use regex::Regex;
use std::fmt;

const EXCLUDE_PATTERNS: [(&'static str, &'static str); 1] = [
  ("bash", r"[[:cntrl:]]\[([0-9]{1,2};)?([0-9]{1,2})?m"),
];

const PATTERNS: [(&'static str, &'static str); 10] = [
  ("url", r"((https?://|git@|git://|ssh://|ftp://|file:///)[\w?=%/_.:,;~@!#$&()*+-]*)"),
  ("diff_a", r"--- a/([^ ]+)"),
  ("diff_b", r"\+\+\+ b/([^ ]+)"),
  ("path", r"[^ ]+/[^ [[:cntrl:]]]+"),
  ("color", r"#[0-9a-fA-F]{6}"),
  ("uid", r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"),
  ("sha", r"[0-9a-f]{7,40}"),
  ("ip", r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}"),
  ("address", r"0x[0-9a-fA-F]+"),
  ("number", r"[0-9]{4,}"),
];

#[derive(Clone)]
pub struct Match<'a> {
  pub x: i32,
  pub y: i32,
  pub text: &'a str,
  pub hint: Option<String>
}

impl<'a> fmt::Debug for Match<'a> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "Match {{ x: {}, y: {}, text: {}, hint: <{}> }}", self.x, self.y, self.text, self.hint.clone().unwrap_or("<undefined>".to_string()))
  }
}

impl<'a> PartialEq for Match<'a> {
  fn eq(&self, other: &Match) -> bool {
    self.x == other.x && self.y == other.y
  }
}

pub struct State<'a> {
  pub lines: &'a Vec<&'a str>,
  alphabet: &'a str,
  regexp: &'a Vec<&'a str>,
}

impl<'a> State<'a> {
  pub fn new(lines: &'a Vec<&'a str>, alphabet: &'a str, regexp: &'a Vec<&'a str>) -> State<'a> {
    State{
      lines: lines,
      alphabet: alphabet,
      regexp: regexp
    }
  }

  pub fn matches(&self, reverse: bool, unique: bool) -> Vec<Match<'a>> {
    let mut matches = Vec::new();

    let exclude_patterns = EXCLUDE_PATTERNS.iter().map(|tuple|
      (tuple.0, Regex::new(tuple.1).unwrap())
    ).collect::<Vec<_>>();

    let custom_patterns = self.regexp.iter().map(|regexp|
      ("custom", Regex::new(regexp).expect("Invalid custom regexp"))
    ).collect::<Vec<_>>();

    let patterns = PATTERNS.iter().map(|tuple|
      (tuple.0, Regex::new(tuple.1).unwrap())
    ).collect::<Vec<_>>();

    let all_patterns = [exclude_patterns, custom_patterns, patterns].concat();

    for (index, line) in self.lines.iter().enumerate() {
      let mut chunk: &str = line;
      let mut offset: i32 = 0;

      loop {
        let submatches = all_patterns.iter().filter_map(|tuple|
          match tuple.1.find_iter(chunk).nth(0) {
            Some(m) => Some((tuple.0, tuple.1.clone(), m)),
            None => None
          }
        ).collect::<Vec<_>>();
        let first_match_option = submatches.iter().min_by(|x, y| x.2.start().cmp(&y.2.start()));

        if let Some(first_match) = first_match_option {
          let (name, pattern, matching) = first_match;
          let text = matching.as_str();

          if let Some(captures) = pattern.captures(text) {
            let (subtext, substart) = if let Some(capture) = captures.get(1) {
              (capture.as_str(), capture.start())
            } else {
              (matching.as_str(), 0)
            };

            // Never hint or broke bash color sequences
            if *name != "bash" {
              matches.push(Match{
                x: offset + matching.start() as i32 + substart as i32,
                y: index as i32,
                text: subtext,
                hint: None
              });
            }

            chunk = chunk.get(matching.end()..).expect("Unknown chunk");
            offset = offset + matching.end() as i32;

          } else {
            panic!("No matching?");
          }
        } else {
          break;
        }
      }
    }

    let alphabet = super::alphabets::get_alphabet(self.alphabet);
    let mut hints = alphabet.hints(matches.len());

    // This looks wrong but we do a pop after
    hints.reverse();

    if reverse {
      matches.reverse();
    }

    if unique {
      let mut previous: HashMap<&str, String> = HashMap::new();

      for mat in &mut matches {
        if let Some(previous_hint) = previous.get(mat.text) {
          mat.hint = Some(previous_hint.clone());
        } else if let Some(hint) = hints.pop() {
          mat.hint = Some(hint.to_string().clone());
          previous.insert(mat.text, hint.to_string().clone());
        }
      }
    } else {
      for mat in &mut matches {
        if let Some(hint) = hints.pop() {
          mat.hint = Some(hint.to_string().clone());
        }
      }
    }

    if reverse {
      matches.reverse();
    }

    return matches;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn split(output: &str) -> Vec<&str> {
    output.split("\n").collect::<Vec<&str>>()
  }

  #[test]
  fn match_reverse () {
    let lines = split("lorem 127.0.0.1 lorem 255.255.255.255 lorem 127.0.0.1 lorem");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 3);
    assert_eq!(results.first().unwrap().hint.clone().unwrap(), "a");
    assert_eq!(results.last().unwrap().hint.clone().unwrap(), "c");
  }

  #[test]
  fn match_unique () {
    let lines = split("lorem 127.0.0.1 lorem 255.255.255.255 lorem 127.0.0.1 lorem");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, true);

    assert_eq!(results.len(), 3);
    assert_eq!(results.first().unwrap().hint.clone().unwrap(), "a");
    assert_eq!(results.last().unwrap().hint.clone().unwrap(), "a");
  }

  #[test]
  fn match_bash () {
    let lines = split("path: [32m/var/log/nginx.log[m\npath: [32mtest/log/nginx.log[m");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 2);
  }

  #[test]
  fn match_paths () {
    let lines = split("Lorem /tmp/foo/bar lorem\n Lorem /var/log/bootstrap.log lorem ../log/kern.log lorem");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 3);
  }

  #[test]
  fn match_uids () {
    let lines = split("Lorem ipsum 123e4567-e89b-12d3-a456-426655440000 lorem\n Lorem lorem lorem");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 1);
  }

  #[test]
  fn match_shas () {
    let lines = split("Lorem fd70b5695 5246ddf f924213 lorem\n Lorem 973113963b491874ab2e372ee60d4b4cb75f717c lorem");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 4);
  }

  #[test]
  fn match_ips () {
    let lines = split("Lorem ipsum 127.0.0.1 lorem\n Lorem 255.255.10.255 lorem 127.0.0.1 lorem");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 3);
  }

  #[test]
  fn match_urls () {
    let lines = split("Lorem ipsum https://www.rust-lang.org/tools lorem\n Lorem https://crates.io lorem https://github.io lorem ssh://github.io");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 4);
  }

  #[test]
  fn match_addresses () {
    let lines = split("Lorem 0xfd70b5695 0x5246ddf lorem\n Lorem 0x973113 lorem");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 3);
  }

  #[test]
  fn match_hex_colors () {
    let lines = split("Lorem #fd7b56 lorem #FF00FF\n Lorem #00fF05 lorem #abcd00 lorem #afRR00");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 4);
  }

  #[test]
  fn match_process_port () {
    let lines = split("Lorem 5695 52463 lorem\n Lorem 973113 lorem 99999 lorem 8888 lorem\n   23456 lorem 5432 lorem 23444");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 8);
  }

  #[test]
  fn match_diff_a () {
    let lines = split("Lorem lorem\n--- a/src/main.rs");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 1);
    assert_eq!(results.first().unwrap().text.clone(), "src/main.rs");
  }

  #[test]
  fn match_diff_b () {
    let lines = split("Lorem lorem\n+++ b/src/main.rs");
    let custom = [].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    assert_eq!(results.len(), 1);
    assert_eq!(results.first().unwrap().text.clone(), "src/main.rs");
  }

  #[test]
  fn priority () {
    let lines = split("Lorem CUSTOM-52463 lorem ISSUE-123 lorem\nLorem /var/fd70b569/9999.log 52463 lorem\n Lorem 973113 lorem 123e4567-e89b-12d3-a456-426655440000 lorem 8888 lorem\n  https://crates.io/23456/fd70b569 lorem");
    let custom = ["CUSTOM-[0-9]{4,}", "ISSUE-[0-9]{3}"].to_vec();
    let results = State::new(&lines, "abcd", &custom).matches(false, false);

    // Matches
    // CUSTOM-52463
    // ISSUE-123
    // /var/fd70b569/9999.log
    // 52463
    // 973113
    // 123e4567-e89b-12d3-a456-426655440000
    // 8888
    // https://crates.io/23456/fd70b569
    assert_eq!(results.len(), 8);
    assert_eq!(results.get(0).unwrap().text.clone(), "CUSTOM-52463");
    assert_eq!(results.get(1).unwrap().text.clone(), "ISSUE-123");
  }
}

//! Read integers from standard input and print their mean.

use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("could not read input");
        return ExitCode::from(1);
    }

    let numbers = match parse_integers(&input) {
        Ok(numbers) => numbers,
        Err(token) => {
            eprintln!("{token} is not an integer");
            return ExitCode::from(1);
        }
    };

    match mean(&numbers) {
        Some(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("enter at least one integer");
            ExitCode::from(1)
        }
    }
}

/// Split `input` on whitespace and parse each piece as an integer.
fn parse_integers(input: &str) -> Result<Vec<i64>, &str> {
    let mut numbers = Vec::new();
    for token in input.split_whitespace() {
        match token.parse::<i64>() {
            Ok(number) => numbers.push(number),
            Err(_) => return Err(token),
        }
    }
    Ok(numbers)
}

/// Arithmetic mean. `None` when there are no numbers.
fn mean(numbers: &[i64]) -> Option<f64> {
    if numbers.is_empty() {
        return None;
    }
    let sum: i128 = numbers.iter().map(|number| *number as i128).sum();
    Some(sum as f64 / numbers.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_of_several_numbers() {
        assert_eq!(mean(&[1, 2, 3, 4]), Some(2.5));
    }

    #[test]
    fn mean_of_one_number() {
        assert_eq!(mean(&[7]), Some(7.0));
    }

    #[test]
    fn mean_of_negatives() {
        assert_eq!(mean(&[-2, 0, 2]), Some(0.0));
    }

    #[test]
    fn empty_input_has_no_mean() {
        assert_eq!(mean(&[]), None);
        assert_eq!(parse_integers("   \n"), Ok(vec![]));
    }

    #[test]
    fn spaces_and_newlines_both_count() {
        assert_eq!(parse_integers("1 2\n3\t4"), Ok(vec![1, 2, 3, 4]));
    }

    #[test]
    fn a_word_is_rejected() {
        assert_eq!(parse_integers("1 two 3"), Err("two"));
    }
}

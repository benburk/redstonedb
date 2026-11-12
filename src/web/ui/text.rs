//! Numbers and words as pages say them.

/// How many people favourited a build, said in words.
pub(crate) fn favourites_label(count: i64) -> String {
    match count {
        1 => "Favourited by 1 person".to_owned(),
        n => format!("Favourited by {n} people"),
    }
}

/// `n` with its thousands separated by commas, such as `131,072`.
pub(crate) fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Tenths of a percent as a whole percent, such as `98%`.
pub(crate) fn percent(permille: u32) -> String {
    format!("{}%", permille / 10)
}

/// `word` for one of them, else with an `s`, such as `Author` and
/// `Authors`.
pub(crate) fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

/// `n` of `word`, such as `1 block` or `3 blocks`.
pub(crate) fn count(n: usize, word: &str) -> String {
    format!("{n} {}", plural(n, word))
}

/// The day of a time such as `2022-07-01T10:30:00Z`: `2022-07-01`.
pub(crate) fn day(time: &str) -> &str {
    time.get(..10).unwrap_or(time)
}

/// `n` game ticks, such as `1 tick` or `3 ticks`.
pub(crate) fn ticks(n: i64) -> String {
    if n == 1 {
        "1 tick".to_owned()
    } else {
        format!("{n} ticks")
    }
}

/// A date such as `2022-07-01`, or a time starting with one, as `July 1,
/// 2022`; anything else as it is.
pub(crate) fn long_date(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let mut parts = day(date).split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return date.to_owned();
    };
    let month = month
        .parse::<usize>()
        .ok()
        .and_then(|m| MONTHS.get(m.checked_sub(1)?));
    match (month, day.parse::<u32>()) {
        (Some(month), Ok(day)) => format!("{month} {day}, {year}"),
        _ => date.to_owned(),
    }
}

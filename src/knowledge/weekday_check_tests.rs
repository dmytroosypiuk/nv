use super::*;

fn wrong(text: &str) -> Option<(String, String, &'static str)> {
    find_wrong_weekday(text).map(|found| (found.said, found.date.to_string(), found.actual))
}

#[test]
fn finds_a_weekday_next_to_a_date_of_another_day() {
    // 2026-10-09 is a Friday, 2026-10-10 a Saturday, 2026-10-11 a Sunday.
    for (text, said) in [
        (
            "Anna reviews the PR by Friday 2026-10-10.",
            "Friday 2026-10-10",
        ),
        ("on friday, 2026-10-10 we meet", "friday, 2026-10-10"),
        ("by Fri 2026-10-10", "Fri 2026-10-10"),
        ("by Friday (2026-10-10)", "Friday (2026-10-10)"),
        ("planned for 2026-10-10 (Friday)", "2026-10-10 (Friday)"),
        ("planned for 2026-10-10 (Fri)", "2026-10-10 (Fri)"),
        ("Fri. 2026-10-10", "Fri. 2026-10-10"),
    ] {
        let found = wrong(text).unwrap_or_else(|| panic!("not found in: {text}"));
        assert_eq!(
            found,
            (said.to_string(), "2026-10-10".to_string(), "Saturday")
        );
    }
    assert_eq!(
        wrong("Monday 2026-10-11").unwrap().2,
        "Sunday",
        "the day it really is"
    );
}

#[test]
fn allows_a_weekday_that_matches_its_date_or_stands_alone() {
    for text in [
        "Anna reviews the PR by Friday 2026-10-09.",
        "Fri, 2026-10-09",
        "2026-10-09 (Friday)",
        "Saturday (2026-10-10)",
        "Planned Thu 2026-10-08, expires Sat 2026-11-14",
        "She reviews it on Friday. The deadline is 2026-10-10.",
        "Monday and 2026-10-10 are far apart",
        "We sat 2026-10-10 as the deadline",
        "Keep the sun 2026-10-10 out of it",
        "Friday 2026-02-30 is not a date",
        "Friday 2026-10",
        "no date at all, only Friday",
    ] {
        assert_eq!(wrong(text), None, "{text}");
    }
}

#[test]
fn reports_the_first_wrong_weekday_only_when_a_later_one_is_right() {
    let found = wrong("Friday 2026-10-09 is fine, but Sunday 2026-10-10 is not").unwrap();

    assert_eq!(found.0, "Sunday 2026-10-10");
    assert_eq!(found.2, "Saturday");
}

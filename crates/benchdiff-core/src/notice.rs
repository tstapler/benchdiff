//! Notice types: one `⚠️` line each. Rendering lives in the markdown modules.

/// Notice class; variant order is the fixed display order (derived `Ord`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoticeClass {
    Degradation,
    Skipped,
    ReferenceAbsent,
    Invalid,
    Chart,
}

impl NoticeClass {
    /// Every class in display order.
    pub const ALL: [NoticeClass; 5] = [
        NoticeClass::Degradation,
        NoticeClass::Skipped,
        NoticeClass::ReferenceAbsent,
        NoticeClass::Invalid,
        NoticeClass::Chart,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub class: NoticeClass,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(class: NoticeClass, text: &str) -> Notice {
        Notice {
            class,
            text: text.to_string(),
        }
    }

    #[test]
    fn all_should_list_classes_in_ascending_order() {
        assert!(NoticeClass::ALL.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(NoticeClass::ALL.len(), 5);
    }

    #[test]
    fn mixed_notices_should_sort_by_class_and_stay_stable_within_class() {
        let mut v = [
            n(NoticeClass::Chart, "c1"),
            n(NoticeClass::Invalid, "i1"),
            n(NoticeClass::Degradation, "d1"),
            n(NoticeClass::Skipped, "s1"),
            n(NoticeClass::Degradation, "d2"),
            n(NoticeClass::ReferenceAbsent, "r1"),
            n(NoticeClass::Skipped, "s2"),
        ];
        v.sort_by_key(|x| x.class);
        let texts: Vec<&str> = v.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, ["d1", "d2", "s1", "s2", "r1", "i1", "c1"]);
    }
}

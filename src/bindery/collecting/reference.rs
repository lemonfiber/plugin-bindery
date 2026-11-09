//! The reference this adapter names a book, an author or a series by: Bindery's foreign
//! id for it, with the words that find it again, since Bindery reads nothing it does not
//! hold by its foreign id.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

const SEPARATOR: &str = "~";

pub(super) struct Reference {
    pub(super) id: String,
    words: Vec<String>,
}

impl Reference {
    pub(super) fn composed(id: &str, words: &[&str]) -> String {
        std::iter::once(id.to_owned())
            .chain(words.iter().map(|word| URL_SAFE_NO_PAD.encode(word)))
            .collect::<Vec<_>>()
            .join(SEPARATOR)
    }

    pub(super) fn read(text: &str, count: usize) -> Option<Self> {
        let mut parts: Vec<&str> = text.rsplitn(count + 1, SEPARATOR).collect();
        parts.reverse();
        let mut parts = parts.into_iter();
        let id = parts.next().filter(|id| !id.is_empty())?.to_owned();
        let words = parts
            .map(|word| {
                URL_SAFE_NO_PAD
                    .decode(word)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
            })
            .collect::<Option<Vec<_>>>()?;
        (words.len() == count).then_some(Self { id, words })
    }

    pub(super) fn word(&self, at: usize) -> &str {
        self.words.get(at).map_or("", String::as_str)
    }
}

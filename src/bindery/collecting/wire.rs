//! Bindery's books, authors and series as its searches and its library answer them.

use serde::Deserialize;

pub(super) const IMPORTED: &str = "imported";

pub(super) const WANTED: &str = "wanted";

pub(super) const SKIPPED: &str = "skipped";

const PUBLISHED: [&str; 2] = ["https://", "http://"];

const PROXIED: &str = "/api/v1/images?";

const ORIGINAL: &str = "url";

const YEAR_DIGITS: usize = 4;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Credited {
    #[serde(default)]
    pub(super) foreign_author_id: String,
    #[serde(default)]
    pub(super) author_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Book {
    #[serde(default)]
    pub(super) id: i64,
    #[serde(default)]
    pub(super) foreign_book_id: String,
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) release_date: Option<String>,
    #[serde(default)]
    pub(super) image_url: Option<String>,
    #[serde(default)]
    pub(super) author: Option<Credited>,
    #[serde(default)]
    pub(super) author_id: i64,
    #[serde(default)]
    pub(super) library_book_id: Option<i64>,
    #[serde(default)]
    pub(super) status: String,
    #[serde(default)]
    pub(super) monitored: bool,
    #[serde(default)]
    pub(super) excluded: bool,
    #[serde(default)]
    pub(super) media_type: String,
    #[serde(default)]
    pub(super) file_path: String,
    #[serde(default)]
    pub(super) ebook_file_path: String,
    #[serde(default)]
    pub(super) audiobook_file_path: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Author {
    #[serde(default)]
    pub(super) id: i64,
    #[serde(default)]
    pub(super) foreign_author_id: String,
    #[serde(default, rename = "authorName")]
    pub(super) name: String,
    #[serde(default)]
    pub(super) image_url: Option<String>,
    #[serde(default)]
    pub(super) library_author_id: Option<i64>,
    #[serde(default)]
    pub(super) monitored: bool,
    #[serde(default)]
    pub(super) books: Vec<Book>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FoundSeries {
    pub(super) foreign_id: String,
    #[serde(default)]
    pub(super) provider_id: String,
    #[serde(default)]
    pub(super) slug: String,
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) author_name: String,
    #[serde(default)]
    pub(super) book_count: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Linked {
    #[serde(default)]
    pub(super) hardcover_series_id: String,
    #[serde(default)]
    pub(super) hardcover_author_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct InSeries {
    #[serde(default)]
    pub(super) book: Option<Book>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Series {
    pub(super) id: i64,
    #[serde(default)]
    pub(super) foreign_series_id: String,
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) monitored: bool,
    #[serde(default)]
    pub(super) books: Vec<InSeries>,
    #[serde(default)]
    pub(super) hardcover_link: Option<Linked>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Page<T> {
    #[serde(default = "Vec::new")]
    pub(super) items: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Setting {
    #[serde(default)]
    pub(super) value: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct Disabled {
    #[serde(default)]
    pub(super) reason: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct Added {
    #[serde(default, rename = "authorId")]
    pub(super) by: Option<i64>,
    #[serde(default, rename = "existingBookId")]
    pub(super) existing: Option<i64>,
    #[serde(default, rename = "canonicalAuthorId")]
    pub(super) canonical: Option<i64>,
}

impl Book {
    pub(super) fn here(&self) -> bool {
        self.status == IMPORTED
            || (self.status == WANTED
                && [
                    &self.file_path,
                    &self.ebook_file_path,
                    &self.audiobook_file_path,
                ]
                .iter()
                .any(|path| !path.is_empty()))
    }

    pub(super) fn counted(&self) -> bool {
        !self.excluded && self.status != SKIPPED
    }

    pub(super) fn by(&self) -> Option<&str> {
        self.author
            .as_ref()
            .map(|by| by.author_name.as_str())
            .filter(|name| !name.is_empty())
    }

    pub(super) fn year(&self) -> Option<u16> {
        self.release_date
            .as_deref()
            .and_then(|date| date.get(..YEAR_DIGITS))
            .filter(|year| year.bytes().all(|digit| digit.is_ascii_digit()))
            .and_then(|year| year.parse().ok())
    }
}

impl Series {
    pub(super) fn is(&self, foreign_id: &str) -> bool {
        self.foreign_series_id == foreign_id
            || self
                .hardcover_link
                .as_ref()
                .is_some_and(|link| link.hardcover_series_id == foreign_id)
    }

    pub(super) fn held(&self) -> impl Iterator<Item = &Book> {
        self.books.iter().filter_map(|one| one.book.as_ref())
    }
}

pub(super) fn published(image: Option<&str>) -> Option<String> {
    let image = image?;
    let original = match image.split_once(PROXIED) {
        Some((_, query)) => url::form_urlencoded::parse(query.as_bytes())
            .find(|(field, _)| field == ORIGINAL)
            .map(|(_, value)| value.into_owned())?,
        None => image.to_owned(),
    };
    PUBLISHED
        .iter()
        .any(|scheme| original.starts_with(scheme))
        .then_some(original)
}

#[cfg(test)]
mod tests;

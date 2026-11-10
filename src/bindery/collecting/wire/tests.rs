use super::{published, Book, Credited, InSeries, Linked, Series, IMPORTED, SKIPPED, WANTED};

fn book(status: &str) -> Book {
    Book {
        status: status.to_owned(),
        ..Book::default()
    }
}

#[test]
fn a_cover_is_published_where_its_address_reaches_beyond_the_stack() {
    assert_eq!(
        published(Some("https://covers.openlibrary.org/b/id/1-L.jpg")).as_deref(),
        Some("https://covers.openlibrary.org/b/id/1-L.jpg")
    );
    assert_eq!(
        published(Some(
            "/api/v1/images?url=http%3A%2F%2Fcovers.example%2F1.jpg&size=l"
        ))
        .as_deref(),
        Some("http://covers.example/1.jpg")
    );
    assert_eq!(
        published(Some("/api/v1/images?url=bindery-cover%3A7")),
        None
    );
    assert_eq!(published(Some("/api/v1/images?size=l")), None);
    assert_eq!(published(Some("")), None);
    assert_eq!(published(None), None);
}

#[test]
fn a_book_is_here_imported_or_wanted_with_a_file() {
    assert!(book(IMPORTED).here());
    for file in ["file_path", "ebook_file_path", "audiobook_file_path"] {
        let mut filed = book(WANTED);
        let path = "/books/one.epub".to_owned();
        match file {
            "file_path" => filed.file_path = path,
            "ebook_file_path" => filed.ebook_file_path = path,
            _ => filed.audiobook_file_path = path,
        }
        assert!(filed.here(), "{file}");
    }
    assert!(!book(WANTED).here());
    assert!(!Book {
        ebook_file_path: "/books/one.epub".to_owned(),
        ..book(SKIPPED)
    }
    .here());
}

#[test]
fn a_book_passed_over_or_excluded_counts_toward_nothing() {
    assert!(book(WANTED).counted());
    assert!(!book(SKIPPED).counted());
    assert!(!Book {
        excluded: true,
        ..book(WANTED)
    }
    .counted());
}

#[test]
fn a_book_names_its_author_and_its_year_where_it_has_them() {
    let named = Book {
        author: Some(Credited {
            foreign_author_id: "OL1A".to_owned(),
            author_name: "Frank Herbert".to_owned(),
        }),
        release_date: Some("1965-08-01T00:00:00Z".to_owned()),
        ..Book::default()
    };
    assert_eq!(named.by(), Some("Frank Herbert"));
    assert_eq!(named.year(), Some(1965));
    let blank = Book {
        author: Some(Credited::default()),
        release_date: Some("mid-60s".to_owned()),
        ..Book::default()
    };
    assert_eq!(blank.by(), None);
    assert_eq!(blank.year(), None);
    assert_eq!(Book::default().year(), None);
}

#[test]
fn a_series_is_the_hardcover_series_it_carries_or_is_linked_to() {
    let carried = Series {
        id: 1,
        foreign_series_id: "hc-series:7".to_owned(),
        ..Series::default()
    };
    let linked = Series {
        id: 2,
        foreign_series_id: "manual:series:9".to_owned(),
        hardcover_link: Some(Linked {
            hardcover_series_id: "hc-series:7".to_owned(),
            hardcover_author_name: String::new(),
        }),
        books: vec![
            InSeries {
                book: Some(book(IMPORTED)),
            },
            InSeries { book: None },
        ],
        ..Series::default()
    };
    assert!(carried.is("hc-series:7"));
    assert!(linked.is("hc-series:7"));
    assert!(!linked.is("hc-series:8"));
    assert!(!Series::default().is("hc-series:7"));
    assert_eq!(linked.held().count(), 1);
}

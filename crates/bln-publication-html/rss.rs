//! Summary RSS from the resolved website scope, not the whole source catalog.
use super::*;
use berlin_document::PublicationDate;

pub(super) fn render(
    assembly: &SiteAssembly,
    site: &PublicationSite,
    options: &SiteHtmlOptions,
    mount: &url::Url,
) -> Result<Vec<u8>> {
    let mut output = BoundedWriter::new(assembly.catalog().limits().output_bytes);
    let mut xml = Xml(&mut output);
    xml.raw("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><channel>\n")?;
    let title = site.title.as_deref().unwrap_or(&assembly.spec().title);
    xml.element("title", title)?;
    xml.element("link", mount.as_str())?;
    xml.element(
        "description",
        options
            .description
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(title),
    )?;
    let feed_url = mount
        .join(RSS_PATH)
        .map_err(|e| failure(Code::UnsafeUrl, e.to_string()))?;
    xml.raw("<atom:link href=\"")?;
    xml.text(feed_url.as_str(), false)?;
    xml.raw("\" rel=\"self\" type=\"application/rss+xml\"/>\n")?;
    let catalog = assembly.catalog();
    let mut entries = assembly
        .document_routes()
        .iter()
        .map(|(id, route)| {
            let entry = catalog.get(id)?;
            Ok((entry, route))
        })
        .collect::<Result<Vec<_>>>()?;
    // Authored dates only, with a stable identity tie-breaker. Undated entries
    // follow dated ones; they never acquire an invented timestamp.
    entries.sort_by(|(a, _), (b, _)| {
        b.descriptor()
            .published
            .cmp(&a.descriptor().published)
            .then_with(|| a.descriptor().id.cmp(&b.descriptor().id))
    });
    for (entry, route) in entries {
        let document = entry.handle().try_local_document()?;
        if document.metadata.draft {
            continue;
        }
        let descriptor = entry.descriptor();
        let url = mount
            .join(route)
            .map_err(|e| failure(Code::UnsafeUrl, e.to_string()))?;
        // No source URIs, private paths, titles, slugs, revision hashes or clock
        // values in the public identity. The site URL scopes ordinary local IDs.
        let identity = fingerprint(
            "rss-document-identity/v1",
            &(mount.as_str(), &descriptor.id),
        )?;
        xml.raw("<item>\n")?;
        xml.element("title", &descriptor.title)?;
        xml.element("link", url.as_str())?;
        xml.raw("<guid isPermaLink=\"false\">urn:berlin:rss:")?;
        xml.text(&identity, false)?;
        xml.raw("</guid>\n")?;
        if let Some(date) = &document.metadata.published {
            xml.element("pubDate", &rss_date(date)?)?;
        }
        if let Some(summary) = descriptor
            .description
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            // RSS descriptions are commonly interpreted as HTML after XML
            // decoding. Encode literal text at both boundaries to keep a
            // metadata string containing markup from becoming active content.
            xml.raw("<description>")?;
            xml.text(summary, true)?;
            xml.raw("</description>\n")?;
        }
        for author in &descriptor.authors {
            xml.element("dc:creator", author)?;
        }
        for tag in &descriptor.topics {
            xml.element("category", tag)?;
        }
        xml.raw("</item>\n")?;
    }
    xml.raw("</channel></rss>\n")?;
    Ok(output.into_bytes())
}

struct Xml<'a>(&'a mut BoundedWriter);
impl Xml<'_> {
    fn raw(&mut self, value: &str) -> Result<()> {
        self.0
            .write_all(value.as_bytes())
            .map_err(|_| failure(Code::ResourceLimit, "RSS output byte budget exceeded"))
    }
    fn element(&mut self, name: &str, value: &str) -> Result<()> {
        self.raw("<")?;
        self.raw(name)?;
        self.raw(">")?;
        self.text(value, false)?;
        self.raw("</")?;
        self.raw(name)?;
        self.raw(">\n")
    }
    fn text(&mut self, value: &str, html: bool) -> Result<()> {
        let mut start = 0;
        for (index, ch) in value.char_indices() {
            if (ch < ' ' && !matches!(ch, '\t' | '\n' | '\r'))
                || matches!(ch, '\u{fffe}' | '\u{ffff}')
            {
                return Err(failure(
                    Code::InvalidProperty,
                    "RSS metadata contains a character forbidden by XML 1.0",
                ));
            }
            let escaped = match (ch, html) {
                ('&', false) => "&amp;",
                ('<', false) => "&lt;",
                ('>', false) => "&gt;",
                ('\"', false) => "&quot;",
                ('\'', false) => "&apos;",
                ('&', true) => "&amp;amp;",
                ('<', true) => "&amp;lt;",
                ('>', true) => "&amp;gt;",
                _ => continue,
            };
            self.raw(&value[start..index])?;
            self.raw(escaped)?;
            start = index + ch.len_utf8();
        }
        self.raw(&value[start..])
    }
}

fn rss_date(date: &PublicationDate) -> Result<String> {
    let value = date.as_str();
    let year: i64 = value[..4].parse().expect("validated publication year");
    let month: usize = value[5..7].parse().expect("validated publication month");
    let day: i64 = value[8..].parse().expect("validated publication day");
    if year < 1900 {
        return Err(failure(
            Code::InvalidProperty,
            "RSS publication dates require a year of at least 1900",
        ));
    }
    let leap = year % 400 == 0 || (year % 4 == 0 && year % 100 != 0);
    let before_month = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][month - 1];
    let previous = year - 1;
    let days = previous * 365 + previous / 4 - previous / 100
        + previous / 400
        + before_month
        + i64::from(leap && month > 2)
        + day
        - 1;
    let weekday = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][(days % 7) as usize];
    let name = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][month - 1];
    // The document contract stores a calendar day, not an observed instant.
    // A fixed midnight UTC representation is deterministic on every machine.
    Ok(format!("{weekday}, {day:02} {name} {year:04} 00:00:00 GMT"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_calendar_days_have_correct_rss_dates() {
        for (date, expected) in [
            ("2026-10-31", "Sat, 31 Oct 2026 00:00:00 GMT"),
            ("2026-11-01", "Sun, 01 Nov 2026 00:00:00 GMT"),
            ("2024-02-29", "Thu, 29 Feb 2024 00:00:00 GMT"),
            ("2000-01-01", "Sat, 01 Jan 2000 00:00:00 GMT"),
            ("1900-03-01", "Thu, 01 Mar 1900 00:00:00 GMT"),
        ] {
            assert_eq!(rss_date(&date.parse().unwrap()).unwrap(), expected);
        }
        assert!(rss_date(&"1899-12-31".parse().unwrap()).is_err());
    }
    #[test]
    fn escaping_is_bounded_and_rejects_invalid_xml_characters() {
        let mut sink = BoundedWriter::new(6);
        assert!(Xml(&mut sink).text("&&", false).is_err());
        assert!(sink.len() <= 6);
        for text in ["bad\0text", "bad\u{fffe}text", "bad\u{ffff}text"] {
            assert_eq!(
                Xml(&mut BoundedWriter::new(100))
                    .text(text, false)
                    .unwrap_err()
                    .code,
                Code::InvalidProperty
            );
        }
    }
}

use base64::{Engine, engine::general_purpose::STANDARD};
use mailparse::{DispositionType, MailHeaderMap, ParsedMail};
use serde::{Deserialize, Serialize};

pub const MAX_EMAIL_BYTES: usize = 2_000_000;

/// Display metadata and a byte-preserving copy of the original RFC 822 message.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Email {
    pub subject: String,
    pub sender: String,
    pub recipient: String,
    pub date: String,
    pub attachments: usize,
    pub original: String,
}
impl Email {
    pub fn validate(&self) -> bool {
        [&self.subject, &self.sender, &self.recipient, &self.date]
            .iter()
            .all(|v| v.len() <= 8192)
            && self.attachments <= 1000
            && self.original.len() <= MAX_EMAIL_BYTES.div_ceil(3) * 4
            && STANDARD
                .decode(&self.original)
                .is_ok_and(|b| !b.is_empty() && b.len() <= MAX_EMAIL_BYTES)
    }
}

pub fn import_email(raw: &[u8]) -> Result<(Email, String), &'static str> {
    if raw.is_empty() || raw.len() > MAX_EMAIL_BYTES {
        return Err("Choose an .eml file under 2 MB.");
    }
    let mail = mailparse::parse_mail(raw)
        .map_err(|_| "Could not read this email. Choose an RFC 822 .eml file.")?;
    if !["From", "Subject", "Date", "Message-ID"]
        .iter()
        .any(|h| mail.headers.get_first_value(h).is_some())
    {
        return Err("This file does not contain email headers.");
    }
    let mut attachments = 0;
    let body = body_text(&mail, &mut attachments)?
        .map(|(body, _)| body)
        .unwrap_or_else(|| {
            "No readable text body. Download the original email to view it in your mail app.".into()
        });
    if body.len() > 1_000_000 {
        return Err("Email text is too large. Choose a smaller message.");
    }
    let metadata = Email {
        subject: mail
            .headers
            .get_first_value("Subject")
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "No subject".into()),
        sender: mail.headers.get_first_value("From").unwrap_or_default(),
        recipient: mail.headers.get_first_value("To").unwrap_or_default(),
        date: mail.headers.get_first_value("Date").unwrap_or_default(),
        attachments,
        original: STANDARD.encode(raw),
    };
    if !metadata.validate() {
        return Err("Email headers or attachments exceed the supported limits.");
    }
    Ok((metadata, body.trim().to_string()))
}
fn body_text(
    mail: &ParsedMail<'_>,
    attachments: &mut usize,
) -> Result<Option<(String, bool)>, &'static str> {
    let disposition = mail.get_content_disposition();
    if disposition.disposition == DispositionType::Attachment
        || disposition.params.contains_key("filename")
        || mail.ctype.params.contains_key("name")
        || mail.ctype.mimetype == "message/rfc822"
    {
        *attachments += 1;
        return Ok(None);
    }
    if !mail.subparts.is_empty() {
        let mut bodies = Vec::new();
        for part in &mail.subparts {
            if let Some(body) = body_text(part, attachments)? {
                bodies.push(body);
            }
        }
        if mail.ctype.mimetype == "multipart/alternative" {
            let preferred = bodies.iter().position(|(_, plain)| *plain).unwrap_or(0);
            return Ok((!bodies.is_empty()).then(|| bodies.remove(preferred)));
        }
        let all_plain = bodies.iter().all(|(_, plain)| *plain);
        return Ok((!bodies.is_empty()).then(|| {
            (
                bodies
                    .into_iter()
                    .map(|(text, _)| text)
                    .collect::<Vec<_>>()
                    .join("\n\n"),
                all_plain,
            )
        }));
    }
    match mail.ctype.mimetype.as_str() {
        "text/plain" | "text/html" => {
            let text = mail
                .get_body()
                .map_err(|_| "Email text encoding could not be decoded.")?;
            if text.trim().is_empty() {
                return Ok(None);
            }
            let plain = mail.ctype.mimetype == "text/plain";
            // Pure text conversion cannot execute scripts or fetch remote resources.
            let text = if plain {
                text
            } else {
                html2text::from_read(text.as_bytes(), 72)
                    .map_err(|_| "Could not convert this email to readable text.")?
            };
            Ok(Some((text.trim().into(), plain)))
        }
        _ => {
            *attachments += 1;
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_headers_charset_and_preserves_original_bytes() {
        let raw = b"From: =?UTF-8?B?Sm9zw6k=?= <jose@example.com>\r\nSubject: =?UTF-8?Q?Caf=C3=A9_notes?=\r\nContent-Type: text/plain; charset=iso-8859-1\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nCaf=E9 discussion";
        let (email, body) = import_email(raw).unwrap();
        assert_eq!(email.subject, "Café notes");
        assert!(email.sender.contains("José"));
        assert_eq!(body, "Café discussion");
        assert_eq!(STANDARD.decode(email.original).unwrap(), raw);
    }
    #[test]
    fn prefers_plain_alternative_and_skips_text_attachments() {
        let raw = b"Subject: Plan\nContent-Type: multipart/mixed; boundary=outer\n\n--outer\nContent-Type: multipart/alternative; boundary=inner\n\n--inner\nContent-Type: text/plain\nContent-Transfer-Encoding: base64\n\nSGVsbG8=\n--inner\nContent-Type: text/html\n\n<b>Duplicate</b>\n--inner--\n--outer\nContent-Type: text/plain\nContent-Disposition: attachment; filename=secret.txt\n\nDo not show attachment as body\n--outer--\n";
        let (email, body) = import_email(raw).unwrap();
        assert_eq!(body, "Hello");
        assert_eq!(email.attachments, 1);
    }
    #[test]
    fn mixed_inline_bodies_are_retained() {
        let (_, body) = import_email(b"Subject: Mixed\nContent-Type: multipart/mixed; boundary=x\n\n--x\nContent-Type: text/html\n\n<p>Main message</p>\n--x\nContent-Type: text/plain\n\nInline footer\n--x--\n").unwrap();
        assert!(body.contains("Main message"));
        assert!(body.contains("Inline footer"));
    }
    #[test]
    fn html_is_text_and_remote_content_is_never_rendered() {
        let (_, body) = import_email(b"Subject: HTML\nContent-Type: text/html; charset=utf-8\n\n<p>Hello <b>world</b> &amp; friends.</p><script>alert(1)</script><img src='https://tracking.invalid/pixel'>").unwrap();
        assert!(body.contains("Hello"));
        assert!(body.contains("world"));
        assert!(body.contains("& friends"));
        assert!(!body.contains("<script>"));
        assert!(!body.contains("<img"));
    }
    #[test]
    fn rejects_non_email_oversize_and_bad_encoding() {
        assert!(import_email(b"not an email").is_err());
        assert!(import_email(&vec![b'a'; MAX_EMAIL_BYTES + 1]).is_err());
        assert!(
            import_email(b"Subject: Broken\nContent-Transfer-Encoding: base64\n\n%not-base64%")
                .is_err()
        );
    }
}

use super::{QrContent, VCard};

impl QrContent {
    // R1–R3
    pub fn payload(&self) -> String {
        match self {
            Self::PlainText(text) => text.clone(),
            Self::Hyperlink(url) if url.contains("://") => url.clone(),
            Self::Hyperlink(url) => format!("http://{url}"),
            Self::Email {
                address,
                subject,
                body,
            } => {
                format!("MATMSG:\nTO:{address};\nSUB:{subject};\nBODY:{body};;")
            }
            Self::TextMessage { number, message } => format!("SMSTO:{number}:{message}"),
            Self::VCard(card) => card.payload(),
        }
    }
}

impl VCard {
    fn payload(&self) -> String {
        let mut lines = vec![
            "BEGIN:VCARD".to_owned(),
            "VERSION:3.0".to_owned(),
            format!("N:{};{}", self.last_name, self.first_name),
            format!("FN:{} {}", self.first_name, self.last_name),
        ];
        let address = [
            self.street_address.as_str(),
            &self.city,
            &self.state,
            &self.postal_code,
            &self.country,
        ];
        let address = if address.iter().any(|part| !part.is_empty()) {
            format!(";;{}", address.join(";"))
        } else {
            String::new()
        };
        for (key, value) in [
            ("ORG", &self.organisation),
            ("TITLE", &self.job_title),
            ("TEL;TYPE=CELL", &self.cell_phone),
            ("TEL;TYPE=WORK,VOICE", &self.phone),
            ("ADR;TYPE=WORK", &address),
            ("EMAIL;TYPE=WORK,INTERNET", &self.email),
            ("URL", &self.website),
        ] {
            if !value.is_empty() {
                lines.push(format!("{key}:{value}"));
            }
        }
        lines.push("END:VCARD".to_owned());
        lines.join("\n")
    }
}

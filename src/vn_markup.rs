///convert the vndb weird formatting
#[derive(Debug, Clone, PartialEq)]
pub enum DescriptionPart {
    Text(String),
    Bold(String),
    Italic(String),
    Link { label: String, url: String },
    Spoiler(Vec<DescriptionPart>),
}
///parse the description
pub fn parse_description(description: String) -> Vec<DescriptionPart> {
    let mut parts = Vec::new();
    let mut remaining = description.as_str();
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix("[b]") {
            if let Some(end_index) = rest.find("[/b]") {
                let bold_text = rest[..end_index].to_string();
                parts.push(DescriptionPart::Bold(bold_text));
                remaining = &rest[end_index + "[/b]".len()..];
                continue;
            }
        }
        if let Some(rest) = remaining.strip_prefix("[i]") {
            if let Some(end_index) = rest.find("[/i]") {
                let italic_text = rest[..end_index].to_string();
                parts.push(DescriptionPart::Italic(italic_text));
                remaining = &rest[end_index + "[/i]".len()..];
                continue;
            }
        }
        if let Some(rest) = remaining.strip_prefix("[url=") {
            if let Some(close_bracket_index) = rest.find("]") {
                let url = rest[..close_bracket_index].to_string();
                let after_open_tag = &rest[close_bracket_index + 1..];
                if let Some(end_index) = after_open_tag.find("[/url]") {
                    let label = after_open_tag[..end_index].to_string();
                    parts.push(DescriptionPart::Link { label, url });
                    remaining = &after_open_tag[end_index + "[/url]".len()..];
                    continue;
                }
            }
        }
        if let Some(rest) = remaining.strip_prefix("[spoiler]") {
            if let Some(end_index) = rest.find("[/spoiler]") {
                let spoiler_text = rest[..end_index].to_string();
                let spoiler_parts = parse_description(spoiler_text);
                parts.push(DescriptionPart::Spoiler(spoiler_parts));
                remaining = &rest[end_index + "[/spoiler]".len()..];
                continue;
            }
        }
        let next_tag_index = remaining.find("[").unwrap_or(remaining.len());
        let plain_text = if next_tag_index == 0 {
            remaining[..1].to_string()
        } else {
            remaining[..next_tag_index].to_string()
        };
        let plain_text_len = plain_text.len();
        parts.push(DescriptionPart::Text(plain_text));
        remaining = &remaining[plain_text_len..];
    }
    parts
}

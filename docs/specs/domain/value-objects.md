# VO — Domain Value Objects Specification

- **Prefix**: `VO`
- **Layer**: `domain`
- **Unit under test**: `domain::schema::AttributeId`, `domain::schema::DocumentTypeId`, `domain::system::LocaleId`, `domain::schema::RegexPattern`
- **Format**: `P` (property invariants)
- **Test target**: `domain/tests/vo_value_objects.rs`

---

## Vocabulary

| Term | Meaning / API symbol | Concrete representation |
|---|---|---|
| `AttributeId` | Field and relation attribute identifier | Validated kebab-case newtype |
| `DocumentTypeId` | Schema document type identifier | Validated kebab-case newtype |
| `LocaleId` | System BCP-47 locale code | `[a-z]{2,3}(-[A-Z]{2})?` |
| `RegexPattern` | Validated compiled regular expression | Newtype over `regex::Regex` |

---

## Rules

### VO-01 — AttributeId Kebab-Case Valid Strings Accepted
- **Status**: `Draft`
- **Origin**: `doc:domain::schema::AttributeId`
- **For all**: Strings `s` matching regex `^[a-z][a-z0-9]*(-[a-z0-9]+)*$` with length in `2..=64`.
- **Then**: `AttributeId::try_new(s)` returns `Ok(attr)` where `attr.as_ref() == s.trim()`.
- **Oracle**: Standard kebab-case regex specification.
- **Boundary Examples**:
  - Valid: `"ab"`, `"title"`, `"user-id"`, `"meta-tag-123"`
  - Invalid: `"a"` (too short), `"A-b"` (uppercase), `"a_b"` (underscore), `"-ab"` (leading dash), `"ab-"` (trailing dash), `""` (empty)

### VO-02 — DocumentTypeId Kebab-Case Valid Strings Accepted
- **Status**: `Draft`
- **Origin**: `doc:domain::schema::DocumentTypeId`
- **For all**: Strings `s` matching regex `^[a-z][a-z0-9]*(-[a-z0-9]+)*$` with length in `2..=64`.
- **Then**: `DocumentTypeId::try_new(s)` returns `Ok(id)` where `id.as_ref() == s.trim()`.
- **Oracle**: Standard kebab-case regex specification.
- **Boundary Examples**:
  - Valid: `"article"`, `"blog-post"`, `"page-v2"`
  - Invalid: `"a"`, `"Blog"`, `"blog__post"`, `"blog--post"`

### VO-03 — LocaleId Conforms to BCP-47 Language Codes
- **Status**: `Draft`
- **Origin**: `doc:domain::system::LocaleId`
- **For all**: Strings `s` matching `^[a-z]{2,3}(-[A-Z]{2})?$` with trimmed length <= 16.
- **Then**: `LocaleId::try_new(s)` returns `Ok(locale)` with `locale.as_ref() == s.trim()`.
- **Oracle**: BCP-47 standard subtag pattern.
- **Boundary Examples**:
  - Valid: `"en"`, `"uk"`, `"fr"`, `"es-ES"`, `"en-US"`, `"de-DE"`, `"ast"`
  - Invalid: `"EN"`, `"en_US"`, `"en-us"`, `"english"`, `""`, `"e"`

### VO-04 — RegexPattern Compiles Valid Regex And Rejects Invalid
- **Status**: `Draft`
- **Origin**: `doc:domain::schema::RegexPattern`
- **For all**: Valid regular expression patterns `p`.
- **Then**: `RegexPattern::try_new(p)` succeeds with `Ok(rp)` where `rp.as_str() == p`, and `rp.is_match(text)` matches `regex::Regex::new(p).unwrap().is_match(text)`.
- **Oracle**: Rust `regex::Regex` compilation oracle.
- **Boundary Examples**:
  - Valid: `"^[a-z]+$"`, `"\\d{3}-\\d{2}"`
  - Invalid: `"[a-z"`, `"("`, `"*"` (compilation error)

---

## Open Questions

1. **Q1 (Hyphen Sequences)**: `^[a-z][a-z0-9]*(-[a-z0-9]+)*$` strictly forbids double hyphens (`"foo--bar"`). Confirmed that double hyphens are invalid for slug/attribute cleanliness.

---

## Findings

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `domain/src/schema/attributes.rs:19` | `len_char_min = 2` | Single character attributes (`"a"`) are rejected | Intended design for semantic clarity |
| 2 | `domain/src/system/locale.rs:13` | Locale regex requires uppercase region code `-[A-Z]{2}` | Case-sensitive region tag | Enforces standard ISO casing |

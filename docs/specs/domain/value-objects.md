# VO — Domain Value Objects Specification

- **Prefix**: `VO`
- **Layer**: `domain`
- **Unit under test**: `domain::schema::AttributeId`, `domain::schema::DocumentTypeId`, `domain::system::LocaleId`, `domain::schema::RegexPattern`, `domain::auth::UserId`, `domain::auth::ServiceId`, `domain::auth::ActorId`
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
| `UserId` | User identifier | Trimmed non-empty string `<= 255` |
| `ServiceId` | Machine/service client identifier | Trimmed non-empty string `<= 245` |
| `ActorId` | Attribution principal identity (`User`, `Service`, `System`) | Tagged enum with compact URN serialization |

---

## Rules

### VO-01 — AttributeId Kebab-Case Valid Strings Accepted
- **Status**: `Approved`
- **Origin**: `doc:domain::schema::AttributeId`
- **For all**: Strings `s` matching regex `^[a-z][a-z0-9]*(-[a-z0-9]+)*$` with length in `2..=64`.
- **Then**: `AttributeId::try_new(s)` returns `Ok(attr)` where `attr.as_ref() == s.trim()`.
- **Oracle**: Standard kebab-case regex specification.
- **Boundary Examples**:
  - Valid: `"ab"`, `"title"`, `"user-id"`, `"meta-tag-123"`
  - Invalid: `"a"` (too short), `"A-b"` (uppercase), `"a_b"` (underscore), `"-ab"` (leading dash), `"ab-"` (trailing dash), `""` (empty)

### VO-02 — DocumentTypeId Kebab-Case Valid Strings Accepted
- **Status**: `Approved`
- **Origin**: `doc:domain::schema::DocumentTypeId`
- **For all**: Strings `s` matching regex `^[a-z][a-z0-9]*(-[a-z0-9]+)*$` with length in `2..=64`.
- **Then**: `DocumentTypeId::try_new(s)` returns `Ok(id)` where `id.as_ref() == s.trim()`.
- **Oracle**: Standard kebab-case regex specification.
- **Boundary Examples**:
  - Valid: `"article"`, `"blog-post"`, `"page-v2"`
  - Invalid: `"a"`, `"Blog"`, `"blog__post"`, `"blog--post"`

### VO-03 — LocaleId Conforms to BCP-47 Language Codes
- **Status**: `Approved`
- **Origin**: `doc:domain::system::LocaleId`
- **For all**: Strings `s` matching `^[a-z]{2,3}(-[A-Z]{2})?$` with trimmed length <= 16.
- **Then**: `LocaleId::try_new(s)` returns `Ok(locale)` with `locale.as_ref() == s.trim()`.
- **Oracle**: BCP-47 standard subtag pattern.
- **Boundary Examples**:
  - Valid: `"en"`, `"uk"`, `"fr"`, `"es-ES"`, `"en-US"`, `"de-DE"`, `"ast"`
  - Invalid: `"EN"`, `"en_US"`, `"en-us"`, `"english"`, `""`, `"e"`

### VO-04 — RegexPattern Compiles Valid Regex And Rejects Invalid
- **Status**: `Approved`
- **Origin**: `doc:domain::schema::RegexPattern`
- **For all**: Valid regular expression patterns `p`.
- **Then**: `RegexPattern::try_new(p)` succeeds with `Ok(rp)` where `rp.as_str() == p`, and `rp.is_match(text)` matches `regex::Regex::new(p).unwrap().is_match(text)`.
- **Oracle**: Rust `regex::Regex` compilation oracle.
- **Boundary Examples**:
  - Valid: `"^[a-z]+$"`, `"\\d{3}-\\d{2}"`
  - Invalid: `"[a-z"`, `"("`, `"*"` (compilation error)

### VO-05 — UserId and ServiceId Length and Non-Empty Invariants
- **Status**: `Approved`
- **Origin**: `adr:004`
- **For all**: Non-empty trimmed strings `u` of length `1..=255` and `s` of length `1..=245`.
- **Then**: `UserId::try_new(u)` and `ServiceId::try_new(s)` succeed, whereas empty/whitespace strings or strings exceeding the max length (`255` for `UserId`, `245` for `ServiceId`) are rejected.

### VO-06 — ActorId URN Serialization Roundtrip
- **Status**: `Approved`
- **Origin**: `adr:004`
- **For all**: Valid `ActorId` values (`ActorId::User(u)`, `ActorId::Service(s)`, `ActorId::System`).
- **Then**: `ActorId::from_urn(&actor.to_urn()) == Ok(actor)` using compact URN format (`"user:<id>"`, `"service:<id>"`, `"system"`), and unrecognized prefixes or empty IDs return `Err(DomainError::Validation(_))`.

---

## Open Questions

1. **Q1 (Hyphen Sequences)**: `^[a-z][a-z0-9]*(-[a-z0-9]+)*$` strictly forbids double hyphens (`"foo--bar"`). Confirmed that double hyphens are invalid for slug/attribute cleanliness.

---

## Findings

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `domain/src/schema/attributes.rs:19` | `len_char_min = 2` | Single character attributes (`"a"`) are rejected | Intended design for semantic clarity |
| 2 | `domain/src/system/locale.rs:13` | Locale regex requires uppercase region code `-[A-Z]{2}` | Case-sensitive region tag | Enforces standard ISO casing |

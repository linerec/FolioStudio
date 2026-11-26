//! Data Merge computed fields: a field whose value an expression makes from the columns of the data source (the language and its
//! rules are in `folio_idml::datamerge_expr`; the list is kept in the document label `folio.datamerge`, key `computed`).
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputedField {
    /// The field's name; a leading `@` makes it an image field and `#` a QR code field (as in a data source's header).
    pub name: String,
    pub expr: String,
}

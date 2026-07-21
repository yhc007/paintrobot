//! Paint recipes: insert/lookup/scan against paintrobot.recipes.
//!
//! One row per (edge, model_no); re-posting the same model upserts the latest
//! recipe (CoreDB overwrites by primary key). The full parameter set is stored
//! as compact JSON in `recipe_json` — CoreDB has no list type.

use crate::{
    check_identifier, decode_i64, decode_text, quote_text, CoreDbClient, HttpTransport, RepoError,
};

#[derive(Debug, Clone)]
pub struct RecipeRow {
    pub event_id: String,
    pub edge_id: String,
    pub model_no: i64,
    pub model_name: String,
    pub levels: i64,
    /// Compact JSON of the `RecipeSet` (atomization/pattern/flow × table/applied).
    pub recipe_json: String,
    pub received_at: i64,
    pub work_date: String,
}

impl<T: HttpTransport> CoreDbClient<T> {
    pub async fn insert_recipe(&self, row: &RecipeRow) -> Result<(), RepoError> {
        check_identifier(&row.event_id)?;
        check_identifier(&row.edge_id)?;
        check_identifier(&row.work_date)?;
        // model_name and recipe_json are free text, not identifiers: model_name
        // may be a localized string, recipe_json holds braces/brackets. Both are
        // embedded via quote_text (escapes quotes). recipe_json is produced by
        // serde_json::to_string — compact, space-free, ASCII — so CoreDB's
        // whitespace-fragile parser sees it as one opaque quoted literal.
        let cql = format!(
            "INSERT INTO {ks}.recipes \
             (event_id, edge_id, model_no, model_name, levels, recipe_json, received_at, work_date) \
             VALUES ({eid}, {edge}, {mno}, {mname}, {lv}, {rj}, {ts}, {wd})",
            ks = self.keyspace,
            eid = quote_text(&row.event_id),
            edge = quote_text(&row.edge_id),
            mno = row.model_no,
            mname = quote_text(&row.model_name),
            lv = row.levels,
            rj = quote_text(&row.recipe_json),
            ts = row.received_at,
            wd = quote_text(&row.work_date),
        );
        self.execute(&cql).await?;
        Ok(())
    }

    pub async fn get_recipe(&self, event_id: &str) -> Result<Option<RecipeRow>, RepoError> {
        check_identifier(event_id)?;
        let cql = format!(
            "SELECT event_id, edge_id, model_no, model_name, levels, recipe_json, received_at, work_date \
             FROM {ks}.recipes WHERE event_id={id}",
            ks = self.keyspace,
            id = quote_text(event_id),
        );
        let rows = self.execute(&cql).await?;
        match rows.first() {
            Some(row) => Ok(Some(decode_recipe(row)?)),
            None => Ok(None),
        }
    }

    /// Scan every recipe currently active on the given work_date. Full scan —
    /// CoreDB has no secondary index.
    pub async fn scan_recipes_for_date(
        &self,
        work_date: &str,
        limit: u32,
    ) -> Result<Vec<RecipeRow>, RepoError> {
        check_identifier(work_date)?;
        let cql = format!(
            "SELECT event_id, edge_id, model_no, model_name, levels, recipe_json, received_at, work_date \
             FROM {ks}.recipes WHERE work_date={wd} LIMIT {n}",
            ks = self.keyspace,
            wd = quote_text(work_date),
            n = limit,
        );
        let rows = self.execute(&cql).await?;
        rows.iter().map(decode_recipe).collect()
    }
}

fn decode_recipe(row: &crate::RawRow) -> Result<RecipeRow, RepoError> {
    let cols = &row.columns;
    let get = |name: &str| {
        cols.get(name)
            .ok_or_else(|| RepoError::Decode(format!("column {name} missing")))
    };
    Ok(RecipeRow {
        event_id: decode_text(get("event_id")?)?,
        edge_id: decode_text(get("edge_id")?)?,
        model_no: decode_i64(get("model_no")?)?,
        model_name: decode_text(get("model_name")?)?,
        levels: decode_i64(get("levels")?)?,
        recipe_json: decode_text(get("recipe_json")?)?,
        received_at: decode_i64(get("received_at")?)?,
        work_date: decode_text(get("work_date")?)?,
    })
}

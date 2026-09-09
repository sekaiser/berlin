//! Simple rectangular tables. Spans and nested block cells fail explicitly.
use super::*;
use berlin_document::{Table, TableAlignment, TableRow};

impl Importer<'_> {
    pub(super) fn table(&mut self, value: &Value) -> Result<Vec<Block>, Error> {
        let [attr, caption, columns, head, bodies, foot] = parts(value)?;
        empty_attr(attr)?;
        let mut alignments = Vec::new();
        for column in array(columns)? {
            let [align, width] = parts(column)?;
            if tag(width)? != "ColWidthDefault" {
                return Err(unsupported("fixed-width table columns"));
            }
            alignments.push(match tag(align)? {
                "AlignDefault" => TableAlignment::None,
                "AlignLeft" => TableAlignment::Left,
                "AlignRight" => TableAlignment::Right,
                "AlignCenter" => TableAlignment::Center,
                other => return Err(unsupported(format!("table alignment '{other}'"))),
            });
        }
        let [head_attr, head_rows] = parts(head)?;
        empty_attr(head_attr)?;
        let mut rows = self.table_rows(array(head_rows)?, true, alignments.len())?;
        for body in array(bodies)? {
            let [attr, row_headers, intermediate_head, body_rows] = parts(body)?;
            empty_attr(attr)?;
            if row_headers.as_u64() != Some(0) || !array(intermediate_head)?.is_empty() {
                return Err(unsupported("table row headers or intermediate headers"));
            }
            rows.extend(self.table_rows(array(body_rows)?, false, alignments.len())?);
        }
        let [foot_attr, foot_rows] = parts(foot)?;
        empty_attr(foot_attr)?;
        if !array(foot_rows)?.is_empty() {
            return Err(unsupported("table foot rows"));
        }
        let [short_caption, caption] = parts(caption)?;
        if !short_caption.is_null() {
            return Err(unsupported("short table captions"));
        }
        let mut blocks = self.blocks(array(caption)?)?;
        blocks.push(Block::Table(Table { alignments, rows }));
        Ok(blocks)
    }

    fn table_rows(
        &mut self,
        rows: &[Value],
        header: bool,
        columns: usize,
    ) -> Result<Vec<TableRow>, Error> {
        rows.iter()
            .map(|row| {
                let [attr, cells] = parts(row)?;
                empty_attr(attr)?;
                if array(cells)?.len() != columns {
                    return Err(unsupported("non-rectangular table"));
                }
                let cells = array(cells)?
                    .iter()
                    .map(|cell| {
                        let [attr, align, rowspan, colspan, blocks] = parts(cell)?;
                        empty_attr(attr)?;
                        if tag(align)? != "AlignDefault"
                            || rowspan.as_u64() != Some(1)
                            || colspan.as_u64() != Some(1)
                        {
                            return Err(unsupported("table cell spans or alignment overrides"));
                        }
                        let [paragraph] = parts(blocks)?;
                        if !matches!(tag(paragraph)?, "Plain" | "Para") {
                            return Err(unsupported("non-inline table cell"));
                        }
                        self.inlines(array(&paragraph["c"])?)
                    })
                    .collect::<Result<_, Error>>()?;
                Ok(TableRow { header, cells })
            })
            .collect()
    }
}

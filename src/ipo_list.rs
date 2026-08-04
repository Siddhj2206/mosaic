use gpui::*;
use gpui_component::table::*;
use gpui_component::StyledExt;
use mosaic_core::types::{Ipo, IpoStatus};

pub struct IpoTableDelegate {
    pub ipos: Vec<Ipo>,
    pub columns: Vec<Column>,
}

impl IpoTableDelegate {
    pub fn new(ipos: Vec<Ipo>) -> Self {
        Self {
            ipos,
            columns: vec![
                Column::new("company", "Company").width(px(180.)).sortable(),
                Column::new("exchange", "Exch").width(px(60.)).sortable(),
                Column::new("price", "Price").width(px(150.)).sortable(),
                Column::new("status", "Status").width(px(90.)).sortable(),
            ],
        }
    }
}

impl TableDelegate for IpoTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.ipos.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> &Column {
        &self.columns[col_ix]
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        let key = self.columns[col_ix].key.clone();
        match sort {
            ColumnSort::Ascending => match key.as_ref() {
                "company" => self.ipos.sort_by(|a, b| a.company_name.cmp(&b.company_name)),
                "exchange" => self.ipos.sort_by(|a, b| a.exchange.cmp(&b.exchange)),
                "price" => self.ipos.sort_by(|a, b| a.price_band_low.cmp(&b.price_band_low)),
                "status" => self.ipos.sort_by(|a, b| a.status.as_str().cmp(b.status.as_str())),
                _ => {}
            },
            ColumnSort::Descending => match key.as_ref() {
                "company" => self.ipos.sort_by(|a, b| b.company_name.cmp(&a.company_name)),
                "exchange" => self.ipos.sort_by(|a, b| b.exchange.cmp(&a.exchange)),
                "price" => self.ipos.sort_by(|a, b| b.price_band_low.cmp(&a.price_band_low)),
                "status" => self.ipos.sort_by(|a, b| b.status.as_str().cmp(a.status.as_str())),
                _ => {}
            },
            _ => {}
        }
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let col_key = self.columns[col_ix].key.clone();
        let company = self.ipos[row_ix].company_name.clone();
        let exchange = self.ipos[row_ix].exchange.clone();
        let price_low = self.ipos[row_ix].price_band_low;
        let price_high = self.ipos[row_ix].price_band_high;
        let status = self.ipos[row_ix].status.clone();

        match col_key.as_ref() {
            "company" => div()
                .font_bold()
                .text_color(rgb(0xe4e5e7))
                .child(company),
            "exchange" => div()
                .text_color(rgb(0x8b8d91))
                .child(exchange.unwrap_or_default()),
            "price" => {
                let text = match (price_low, price_high) {
                    (Some(l), Some(h)) => format!("\u{20b9}{l} - \u{20b9}{h}"),
                    (Some(l), None) => format!("\u{20b9}{l}"),
                    _ => "-".into(),
                };
                div().text_color(rgb(0x14b8a6)).child(text)
            }
            "status" => render_status_badge(&status),
            _ => div(),
        }
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .text_color(rgb(0x8b8d91))
            .child("No IPOs found")
    }
}

fn render_status_badge(status: &IpoStatus) -> Div {
    let (bg, text, label) = match status {
        IpoStatus::Listed => (rgb(0x166534), rgb(0x4ade80), "Listed"),
        IpoStatus::Open => (rgb(0x1e3a5f), rgb(0x60a5fa), "Open"),
        IpoStatus::Upcoming => (rgb(0x5c3d0e), rgb(0xeab308), "Upcoming"),
        IpoStatus::Closed => (rgb(0x374151), rgb(0x9ca3af), "Closed"),
        IpoStatus::Withdrawn => (rgb(0x5f1f1f), rgb(0xef4444), "Withdrawn"),
    };
    div()
        .px(px(8.))
        .py(px(2.))
        .rounded(px(4.))
        .bg(bg)
        .text_color(text)
        .child(label)
}

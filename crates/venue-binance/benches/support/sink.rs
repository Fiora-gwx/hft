#[derive(Default, Debug)]
pub struct CountSink {
    orders: u64,
    ids: u64,
}
impl hft_venue_proto::ReportSink for CountSink {
    fn order(&mut self, e: &hft_event::OrderEvent) {
        std::hint::black_box(e);
        self.orders += 1;
    }
    fn position(&mut self, _e: &hft_event::PositionReport) {}
    fn balance(&mut self, _e: &hft_event::BalanceReport) {}
    fn funding_settle(&mut self, _e: &hft_event::FundingSettle) {}
    fn external_fill(&mut self, e: &hft_event::ExternalFill) {
        std::hint::black_box(e);
    }
    fn margin(&mut self, _h: &hft_event::MarginReportHdr, _i: &[hft_event::MarginItem]) {}
    fn account_config(&mut self, _h: &hft_event::AccountConfigReportHdr, _l: &[hft_event::LeverageItem]) {}
    fn exch_id(&mut self, key: &[u8], _o: Option<&hft_types::ClientOrderId>) -> hft_venue_proto::ExchIdHint {
        std::hint::black_box(key);
        self.ids += 1;
        hft_venue_proto::ExchIdHint::Recorded
    }
}

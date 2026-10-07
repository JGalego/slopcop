/// Receives fetch events; every callback defaults to doing nothing.
pub trait FetchTaskTarget {
    fn process_response(&mut self, _: &Request, _: &Response) {}
}

/// Discards every fetch event for requests whose response nobody reads.
struct DiscardFetch;

impl FetchTaskTarget for DiscardFetch {
    fn process_response(&mut self, _: &Request, _: &Response) {}
    fn process_response_eof(&mut self, _: &Request, _: &Response) {}
}

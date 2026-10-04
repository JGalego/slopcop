fn store_first() {
    let alpha = transform(first_value);
    let beta = transform(second_value);
    persist(combine(alpha, beta));
}

fn store_second() {
    let record = decode(payload);
    validate(record, expected_schema);
    notify(record, subscription_registry);
}
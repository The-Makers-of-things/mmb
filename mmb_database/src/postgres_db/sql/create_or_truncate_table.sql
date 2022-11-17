DROP TABLE IF EXISTS TABLE_NAME CASCADE;
CREATE TABLE TABLE_NAME(
    id bigint PRIMARY KEY GENERATED BY DEFAULT AS IDENTITY,
    insert_time timestamp WITH TIME ZONE NOT NULL DEFAULT now(),
    version int,
    json jsonb NOT NULL
);

CREATE INDEX IF NOT EXISTS TABLE_NAME_insert_time_idx ON TABLE_NAME USING btree (insert_time);
CREATE INDEX IF NOT EXISTS TABLE_NAME_exchange_id_idx ON TABLE_NAME USING btree (((json ->> 'exchange_id')::text));
CREATE INDEX IF NOT EXISTS TABLE_NAME_currency_pair_idx ON TABLE_NAME USING btree (((json ->> 'currency_pair')::text));

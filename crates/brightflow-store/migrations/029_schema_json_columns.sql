-- One shape for tables.schema_json: the contract's TableSchema
-- ({"columns":[{"name","datatype","nullable","physical"}]}). Rows written
-- before 027 carry the engine's own shape ({"fields":[{"name","type"}]});
-- rewrite them, mapping the Polars type word to a logical type and keeping
-- the word as `physical`. Readers then parse one type, not two.
UPDATE tables
SET schema_json = (
    SELECT json_object('columns', json_group_array(json_object(
        'name', json_extract(f.value, '$.name'),
        'datatype', CASE
            WHEN json_extract(f.value, '$.type') = 'str' THEN 'String'
            WHEN json_extract(f.value, '$.type') IN ('i8','i16','i32','i64','u8','u16','u32','u64') THEN 'Integer'
            WHEN json_extract(f.value, '$.type') IN ('f32','f64') THEN 'Float'
            WHEN json_extract(f.value, '$.type') = 'bool' THEN 'Boolean'
            WHEN json_extract(f.value, '$.type') = 'date' THEN 'Date'
            WHEN json_extract(f.value, '$.type') = 'time' THEN 'Time'
            WHEN json_extract(f.value, '$.type') LIKE 'datetime[%,%' THEN 'DateTimeTz'
            WHEN json_extract(f.value, '$.type') LIKE 'datetime[%' THEN 'DateTime'
            ELSE 'Opaque'
        END,
        'nullable', json('true'),
        'physical', json_extract(f.value, '$.type')
    )))
    FROM json_each(tables.schema_json, '$.fields') AS f
)
WHERE schema_json IS NOT NULL
  AND json_type(schema_json, '$.fields') = 'array';

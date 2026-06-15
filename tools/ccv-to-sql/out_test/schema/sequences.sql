-- Reset sequences to max(id) after the bulk load.
SELECT setval('authors_id_seq', 6);
SELECT setval('poems_id_seq',   425);
SELECT setval('stanzas_id_seq', 1944);
SELECT setval('verses_id_seq',  9476);
SELECT setval('tokens_id_seq',  5315);

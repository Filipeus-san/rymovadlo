-- books
BEGIN;
INSERT INTO books (id, author_id, b_title, b_subtitle, publisher, place, year, edition, pages, signature, motto, motto_aut) VALUES
('0001', 1, 'Na zemi a na nebi', 'Básně', 'Šimáček, František; Unie', 'Praha', '1900', '[1.]', '[XVI]+104', 'Národní knihovna ČR, Praha; 54 H 2287', NULL, NULL),
('0002', 2, 'Nebeský vůdce', 'Básně', 'Papežská knihtiskárna rajhradských benediktinů', 'Brno', '1884', '[1.]', '40', 'MZK – UK Brno; 1-23467', 'Následuj mne!', 'Bible, NZ, Matouš  (Matouš 16, 24.)'),
('0003', 2, 'Pestré kvítí', 'Básně', 'Papežská knihtiskárna rajhradských benediktinů', 'Brno', '1883', '[1.]', '122', 'ÚČL AV ČR, pobočka Brno; R-307 ', NULL, NULL),
('0004', 3, 'Kytice z básní a písní', NULL, 'Baše, Josef; Vaněk, František', 'Valašské Meziříčí', '1883', '[1.]', '64', 'Národní knihovna ČR, Praha; 63 C 487', NULL, NULL),
('0005', 4, 'Básně', NULL, 'Vilímek, Josef Richard', 'Praha', '1892', '[1.]', '78', 'Národní knihovna ČR, Praha; 54 H 842', NULL, NULL),
('0006', 5, 'Verše', NULL, 'Řivnáč, František; Auředníček, Otakar', 'Praha', '1889', '[1.]', '16', 'soukromý zdroj', 'Bénis soient-ils! bénis soient ceux que sacrifie  L’imbécile faveur du vulgaire odieux,  Et qui pensent, et dont la bouche glorifie  Les poëtes sacrés et la race de Dieux.', 'Banville, Théodore de  (Théodore de Banville.)'),
('0007', 5, 'Zpívající labutě', NULL, 'Zábavná knihovna; Popelka, František', 'Polička', '1891', '[1.]', '110', 'Národní knihovna ČR, Praha; 63 C 713', NULL, NULL),
('0008', 6, 'Když slunce zapadá', NULL, 'Weinfurter, Eduard; Stivín, Emanuel', 'Praha', '1900', '[1.]', '35', 'Národní knihovna ČR, Praha; 54 J 2058', NULL, NULL),
('0009', 6, 'Kniha písní', NULL, 'Pospíšil, Jaroslav; Binko, [?]; Zika, [?]', 'Praha', '1908', '[1.]', '80', 'Národní knihovna ČR, Praha; 54 K 10296', NULL, NULL),
('0010', 6, 'Ocúny na lukách', NULL, 'Kotík, Jan; Stiburek, Emil', 'Praha', '1935', '[1.]', '68', 'Národní knihovna ČR, Praha; I 122', NULL, NULL);
COMMIT;

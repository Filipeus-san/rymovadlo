-- authors
BEGIN;
INSERT INTO authors (id, identity, name, born, died) VALUES
(1, 'Albert, Eduard', 'Albert, Eduard', 1841, 1900),
(2, 'Ambrož, Vilém', 'Ambrož, Vilém', 1846, 1903),
(3, 'Baše, Josef', 'Antonowicz, E.', 1850, 1889),
(4, 'Arietto, Ladislav', 'Arietto, Ladislav', 1861, 1927),
(5, 'Auředníček, Otakar', 'Auředníček, Otakar', 1868, 1947),
(6, 'Babánek, Karel', 'Babánek, Karel', 1872, 1937);
COMMIT;

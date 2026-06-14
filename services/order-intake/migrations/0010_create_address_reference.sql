-- Migration 0010: Global address reference table
--
-- Multi-country postal code ↔ city/province lookup.
-- Shared across all services via the `reference` schema.
-- Not tenant-scoped — one row per (country, postal_code, city) combination.
--
-- Bulk-load production data from:
--   PH  → PSA PSGC / PhilPost (https://www.philpost.gov.ph)
--   US  → USPS / GeoNames (https://download.geonames.org/export/zip/)
--   SG  → SingPost / data.gov.sg
--   Others → GeoNames postal code dataset (free, CC-BY)

-- ── Extension ──────────────────────────────────────────────────────────────────
CREATE EXTENSION IF NOT EXISTS pg_trgm;          -- trigram fuzzy search on city

-- ── Schema ────────────────────────────────────────────────────────────────────
CREATE SCHEMA IF NOT EXISTS reference;

-- ── Table ─────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS reference.address_codes (
    id             UUID        NOT NULL DEFAULT gen_random_uuid(),
    country_code   CHAR(2)     NOT NULL,   -- ISO 3166-1 alpha-2: PH, US, SG, GB …
    postal_code    TEXT        NOT NULL,   -- format varies per country
    city           TEXT        NOT NULL,   -- district / municipality / city
    state_province TEXT,                   -- province (PH), state (US), emirate (AE) …
    region         TEXT,                   -- PH Region I-XVII; US Census region; etc.
    time_zone      TEXT,                   -- IANA tz  e.g. "Asia/Manila"
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT uq_address_codes UNIQUE (country_code, postal_code, city)
);

-- ── Indexes ───────────────────────────────────────────────────────────────────

-- Exact postal_code lookup  → GET ?postal_code=1550
CREATE INDEX IF NOT EXISTS idx_addrref_country_postal
    ON reference.address_codes (country_code, postal_code);

-- Prefix city search  → GET ?city=Mak  (type-ahead; text_pattern_ops enables LIKE 'x%')
CREATE INDEX IF NOT EXISTS idx_addrref_city_prefix
    ON reference.address_codes (country_code, lower(city) text_pattern_ops);

-- Trigram fuzzy search  → GET ?q=Makati  (requires pg_trgm extension above)
CREATE INDEX IF NOT EXISTS idx_addrref_city_trgm
    ON reference.address_codes USING gin (lower(city) gin_trgm_ops);

-- ── Grants ────────────────────────────────────────────────────────────────────
GRANT USAGE  ON SCHEMA reference TO PUBLIC;
GRANT SELECT ON reference.address_codes TO PUBLIC;

-- ── Seed Data ─────────────────────────────────────────────────────────────────
-- Replace / extend this with a bulk COPY from GeoNames or PhilPost CSV in production.
-- All rows are idempotent (ON CONFLICT DO NOTHING).

INSERT INTO reference.address_codes
    (country_code, postal_code, city, state_province, region, time_zone)
VALUES
    -- ── Philippines — NCR ────────────────────────────────────────────────────
    ('PH','1000','Manila',          'Metro Manila','NCR','Asia/Manila'),
    ('PH','1001','Ermita',          'Metro Manila','NCR','Asia/Manila'),
    ('PH','1002','Intramuros',      'Metro Manila','NCR','Asia/Manila'),
    ('PH','1003','Quiapo',          'Metro Manila','NCR','Asia/Manila'),
    ('PH','1004','Sampaloc',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1005','Tondo',           'Metro Manila','NCR','Asia/Manila'),
    ('PH','1006','Binondo',         'Metro Manila','NCR','Asia/Manila'),
    ('PH','1007','Malate',          'Metro Manila','NCR','Asia/Manila'),
    ('PH','1008','Paco',            'Metro Manila','NCR','Asia/Manila'),
    ('PH','1009','Pandacan',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1010','San Nicolas',     'Metro Manila','NCR','Asia/Manila'),
    ('PH','1011','Santa Ana',       'Metro Manila','NCR','Asia/Manila'),
    ('PH','1012','Santa Cruz',      'Metro Manila','NCR','Asia/Manila'),
    ('PH','1013','Santa Mesa',      'Metro Manila','NCR','Asia/Manila'),
    ('PH','1100','Quezon City',     'Metro Manila','NCR','Asia/Manila'),
    ('PH','1101','Cubao',           'Metro Manila','NCR','Asia/Manila'),
    ('PH','1102','Diliman',         'Metro Manila','NCR','Asia/Manila'),
    ('PH','1103','Fairview',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1104','Novaliches',      'Metro Manila','NCR','Asia/Manila'),
    ('PH','1105','Project 6',       'Metro Manila','NCR','Asia/Manila'),
    ('PH','1106','Tandang Sora',    'Metro Manila','NCR','Asia/Manila'),
    ('PH','1107','Batasan Hills',   'Metro Manila','NCR','Asia/Manila'),
    ('PH','1108','Commonwealth',    'Metro Manila','NCR','Asia/Manila'),
    ('PH','1109','Kamias',          'Metro Manila','NCR','Asia/Manila'),
    ('PH','1110','Kamuning',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1111','Loyola Heights',  'Metro Manila','NCR','Asia/Manila'),
    ('PH','1119','UP Campus',       'Metro Manila','NCR','Asia/Manila'),
    ('PH','1200','Makati',          'Metro Manila','NCR','Asia/Manila'),
    ('PH','1201','Bel-Air',         'Metro Manila','NCR','Asia/Manila'),
    ('PH','1203','Forbes Park',     'Metro Manila','NCR','Asia/Manila'),
    ('PH','1207','Rockwell',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1208','San Lorenzo',     'Metro Manila','NCR','Asia/Manila'),
    ('PH','1209','Urdaneta',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1210','Poblacion',       'Metro Manila','NCR','Asia/Manila'),
    ('PH','1211','Pembo',           'Metro Manila','NCR','Asia/Manila'),
    ('PH','1300','Caloocan',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1400','Malabon',         'Metro Manila','NCR','Asia/Manila'),
    ('PH','1480','Navotas',         'Metro Manila','NCR','Asia/Manila'),
    ('PH','1490','Valenzuela',      'Metro Manila','NCR','Asia/Manila'),
    ('PH','1500','Pasay',           'Metro Manila','NCR','Asia/Manila'),
    ('PH','1550','Pasig',           'Metro Manila','NCR','Asia/Manila'),
    ('PH','1600','Mandaluyong',     'Metro Manila','NCR','Asia/Manila'),
    ('PH','1630','San Juan',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1700','Paranaque',       'Metro Manila','NCR','Asia/Manila'),
    ('PH','1701','BF Homes',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1740','Las Pinas',       'Metro Manila','NCR','Asia/Manila'),
    ('PH','1750','Muntinlupa',      'Metro Manila','NCR','Asia/Manila'),
    ('PH','1770','Pateros',         'Metro Manila','NCR','Asia/Manila'),
    ('PH','1800','Marikina',        'Metro Manila','NCR','Asia/Manila'),
    ('PH','1900','Taguig',          'Metro Manila','NCR','Asia/Manila'),
    ('PH','1950','BGC',             'Metro Manila','NCR','Asia/Manila'),
    -- ── Philippines — Cebu ───────────────────────────────────────────────────
    ('PH','6000','Cebu City',       'Cebu','Region VII','Asia/Manila'),
    ('PH','6001','Labangon',        'Cebu','Region VII','Asia/Manila'),
    ('PH','6002','Mambaling',       'Cebu','Region VII','Asia/Manila'),
    ('PH','6003','Mandaue',         'Cebu','Region VII','Asia/Manila'),
    ('PH','6004','Lapu-Lapu',       'Cebu','Region VII','Asia/Manila'),
    ('PH','6005','Mactan',          'Cebu','Region VII','Asia/Manila'),
    ('PH','6006','Talisay',         'Cebu','Region VII','Asia/Manila'),
    ('PH','6010','Consolacion',     'Cebu','Region VII','Asia/Manila'),
    ('PH','6014','Liloan',          'Cebu','Region VII','Asia/Manila'),
    ('PH','6015','Compostela',      'Cebu','Region VII','Asia/Manila'),
    -- ── Philippines — Davao ──────────────────────────────────────────────────
    ('PH','8000','Davao City',      'Davao del Sur','Region XI','Asia/Manila'),
    ('PH','8001','Toril',           'Davao del Sur','Region XI','Asia/Manila'),
    ('PH','8002','Mintal',          'Davao del Sur','Region XI','Asia/Manila'),
    ('PH','8003','Calinan',         'Davao del Sur','Region XI','Asia/Manila'),
    -- ── Philippines — Iloilo ─────────────────────────────────────────────────
    ('PH','5000','Iloilo City',     'Iloilo','Region VI','Asia/Manila'),
    ('PH','5001','Mandurriao',      'Iloilo','Region VI','Asia/Manila'),
    -- ── Philippines — Other major cities ─────────────────────────────────────
    ('PH','9000','Cagayan de Oro',  'Misamis Oriental','Region X','Asia/Manila'),
    ('PH','6100','Bacolod',         'Negros Occidental','Region VI','Asia/Manila'),
    ('PH','7000','Zamboanga City',  'Zamboanga del Sur','Region IX','Asia/Manila'),
    ('PH','2600','Baguio',          'Benguet','CAR','Asia/Manila'),
    ('PH','4400','Lucena',          'Quezon','Region IV-A','Asia/Manila'),
    ('PH','3200','Batangas City',   'Batangas','Region IV-A','Asia/Manila'),
    ('PH','2100','Olongapo',        'Zambales','Region III','Asia/Manila'),
    -- ── Philippines — Pampanga ───────────────────────────────────────────────
    ('PH','2000','Angeles',         'Pampanga','Region III','Asia/Manila'),
    ('PH','2001','San Fernando',    'Pampanga','Region III','Asia/Manila'),
    -- ── Philippines — Bulacan ────────────────────────────────────────────────
    ('PH','3000','Malolos',         'Bulacan','Region III','Asia/Manila'),
    ('PH','3001','Meycauayan',      'Bulacan','Region III','Asia/Manila'),
    ('PH','3010','Marilao',         'Bulacan','Region III','Asia/Manila'),
    ('PH','3011','Bocaue',          'Bulacan','Region III','Asia/Manila'),
    -- ── Philippines — Cavite ─────────────────────────────────────────────────
    ('PH','4100','Imus',            'Cavite','Region IV-A','Asia/Manila'),
    ('PH','4101','Bacoor',          'Cavite','Region IV-A','Asia/Manila'),
    ('PH','4102','General Trias',   'Cavite','Region IV-A','Asia/Manila'),
    ('PH','4103','Dasmarinas',      'Cavite','Region IV-A','Asia/Manila'),
    ('PH','4104','Kawit',           'Cavite','Region IV-A','Asia/Manila'),
    -- ── Philippines — Laguna ─────────────────────────────────────────────────
    ('PH','4025','Binan',           'Laguna','Region IV-A','Asia/Manila'),
    ('PH','4026','San Pedro',       'Laguna','Region IV-A','Asia/Manila'),
    ('PH','4027','Santa Rosa',      'Laguna','Region IV-A','Asia/Manila'),
    ('PH','4028','Cabuyao',         'Laguna','Region IV-A','Asia/Manila'),
    ('PH','4031','Calamba',         'Laguna','Region IV-A','Asia/Manila'),
    -- ── Philippines — Rizal ──────────────────────────────────────────────────
    ('PH','1870','Antipolo',        'Rizal','Region IV-A','Asia/Manila'),
    ('PH','1800','Cainta',          'Rizal','Region IV-A','Asia/Manila'),
    ('PH','1850','Taytay',          'Rizal','Region IV-A','Asia/Manila'),
    -- ── Singapore ────────────────────────────────────────────────────────────
    ('SG','018956','Marina Bay',       'Central Region',   'Downtown Core',  'Asia/Singapore'),
    ('SG','048581','Tanjong Pagar',    'Central Region',   'Outram',         'Asia/Singapore'),
    ('SG','238859','Orchard',          'Central Region',   'Novena',         'Asia/Singapore'),
    ('SG','189673','Bugis',            'Central Region',   'Rochor',         'Asia/Singapore'),
    ('SG','307683','Novena',           'Central Region',   'Novena',         'Asia/Singapore'),
    ('SG','408564','Bedok',            'East Region',      'Bedok',          'Asia/Singapore'),
    ('SG','520001','Tampines',         'East Region',      'Tampines',       'Asia/Singapore'),
    ('SG','460001','Pasir Ris',        'East Region',      'Pasir Ris',      'Asia/Singapore'),
    ('SG','570001','Woodlands',        'North Region',     'Woodlands',      'Asia/Singapore'),
    ('SG','760001','Yishun',           'North Region',     'Yishun',         'Asia/Singapore'),
    ('SG','640001','Jurong East',      'West Region',      'Jurong East',    'Asia/Singapore'),
    ('SG','600001','Jurong West',      'West Region',      'Jurong West',    'Asia/Singapore'),
    ('SG','150001','Bukit Merah',      'Central Region',   'Bukit Merah',    'Asia/Singapore'),
    -- ── United States ────────────────────────────────────────────────────────
    ('US','10001','New York',          'New York',         'Northeast',      'America/New_York'),
    ('US','10002','Lower East Side',   'New York',         'Northeast',      'America/New_York'),
    ('US','10007','Downtown Manhattan','New York',         'Northeast',      'America/New_York'),
    ('US','11201','Brooklyn',          'New York',         'Northeast',      'America/New_York'),
    ('US','90001','Los Angeles',       'California',       'West',           'America/Los_Angeles'),
    ('US','90210','Beverly Hills',     'California',       'West',           'America/Los_Angeles'),
    ('US','94102','San Francisco',     'California',       'West',           'America/Los_Angeles'),
    ('US','60601','Chicago',           'Illinois',         'Midwest',        'America/Chicago'),
    ('US','77001','Houston',           'Texas',            'South',          'America/Chicago'),
    ('US','78201','San Antonio',       'Texas',            'South',          'America/Chicago'),
    ('US','85001','Phoenix',           'Arizona',          'West',           'America/Phoenix'),
    ('US','19101','Philadelphia',      'Pennsylvania',     'Northeast',      'America/New_York'),
    ('US','78201','San Antonio',       'Texas',            'South',          'America/Chicago'),
    ('US','92101','San Diego',         'California',       'West',           'America/Los_Angeles'),
    ('US','75201','Dallas',            'Texas',            'South',          'America/Chicago'),
    -- ── United Kingdom ───────────────────────────────────────────────────────
    ('GB','SW1A 1AA','Westminster',    'London',           'England',        'Europe/London'),
    ('GB','EC1A 1BB','City of London', 'London',           'England',        'Europe/London'),
    ('GB','E1 6RF',  'Tower Hamlets',  'London',           'England',        'Europe/London'),
    ('GB','M1 1AE',  'Manchester',     'Greater Manchester','England',       'Europe/London'),
    ('GB','B1 1BB',  'Birmingham',     'West Midlands',    'England',        'Europe/London'),
    ('GB','EH1 1YZ', 'Edinburgh',      'Edinburgh',        'Scotland',       'Europe/London'),
    ('GB','CF10 1EP','Cardiff',        'Cardiff',          'Wales',          'Europe/London'),
    -- ── United Arab Emirates ─────────────────────────────────────────────────
    ('AE','00000','Dubai',             'Dubai',            'Emirate of Dubai',       'Asia/Dubai'),
    ('AE','00001','Abu Dhabi',         'Abu Dhabi',        'Emirate of Abu Dhabi',   'Asia/Dubai'),
    ('AE','00002','Sharjah',           'Sharjah',          'Emirate of Sharjah',     'Asia/Dubai'),
    ('AE','00003','Ajman',             'Ajman',            'Emirate of Ajman',       'Asia/Dubai'),
    -- ── Australia ────────────────────────────────────────────────────────────
    ('AU','2000','Sydney',             'New South Wales',  'Oceania',        'Australia/Sydney'),
    ('AU','3000','Melbourne',          'Victoria',         'Oceania',        'Australia/Melbourne'),
    ('AU','4000','Brisbane',           'Queensland',       'Oceania',        'Australia/Brisbane'),
    ('AU','6000','Perth',              'Western Australia','Oceania',        'Australia/Perth'),
    ('AU','5000','Adelaide',           'South Australia',  'Oceania',        'Australia/Adelaide'),
    -- ── Canada ───────────────────────────────────────────────────────────────
    ('CA','M5H 2N2','Toronto',         'Ontario',          'Ontario',        'America/Toronto'),
    ('CA','H3A 0G4','Montreal',        'Quebec',           'Quebec',         'America/Toronto'),
    ('CA','V6B 1A1','Vancouver',       'British Columbia', 'British Columbia','America/Vancouver')
ON CONFLICT (country_code, postal_code, city) DO NOTHING;

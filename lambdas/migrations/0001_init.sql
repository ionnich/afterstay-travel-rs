-- Reconstructed from lib/types.ts + lib/supabase.ts (snake_case -> camelCase mappers).
-- A live Supabase dump was unavailable; column lists/types are derived from the
-- mapper functions and INSERT/UPDATE payloads in lib/supabase.ts.

CREATE TABLE IF NOT EXISTS profiles (
    id         text PRIMARY KEY,          -- Cognito sub
    full_name  text NOT NULL DEFAULT '',
    avatar_url text,
    phone      text
);

CREATE TABLE IF NOT EXISTS trips (
    id                    uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name                  text NOT NULL,
    destination           text,
    start_date            date NOT NULL,
    end_date              date NOT NULL,
    accommodation_name    text,
    accommodation_address text,
    room_type             text,
    check_in              text,
    check_out             text,
    hotel_phone           text,
    booking_ref           text,
    currency              text DEFAULT 'PHP',
    cover_image           text,
    transport_mode        text,
    wifi_ssid             text,
    wifi_password         text,
    door_code             text,
    notes                 text,
    status                text NOT NULL DEFAULT 'Planning'
        CHECK (status IN ('Planning', 'Active', 'Completed')),
    hotel_url             text,
    airport_arrival_buffer text,
    airport_to_hotel_time text,
    custom_quick_access   text,
    transport_notes       text,
    house_rules           text,
    emergency_contacts    text,
    hotel_photos          text,            -- JSON string of URL array
    budget_limit          numeric,
    budget_mode           text NOT NULL DEFAULT 'Unlimited'
        CHECK (budget_mode IN ('Limited', 'Unlimited')),
    -- Lifetime / past-trip fields
    user_id               text,
    is_past_import        boolean,
    confidence_level      text CHECK (confidence_level IN ('real', 'user_added')),
    date_precision        text CHECK (date_precision IN ('exact', 'month_year')),
    country               text,
    country_code          text,
    latitude              numeric,
    longitude             numeric,
    total_spent           numeric,
    total_nights          numeric
);

CREATE TABLE IF NOT EXISTS trip_members (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id     uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    user_id     text,
    name        text NOT NULL,
    role        text NOT NULL DEFAULT 'Member'
        CHECK (role IN ('Primary', 'Member')),
    phone       text,
    email       text,
    avatar_url  text
);

CREATE TABLE IF NOT EXISTS flights (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id      uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    direction    text NOT NULL DEFAULT 'Outbound'
        CHECK (direction IN ('Outbound', 'Return')),
    flight_number text NOT NULL,
    airline      text,
    from_city    text,
    to_city      text,
    depart_time  timestamptz NOT NULL,
    arrive_time  timestamptz NOT NULL,
    booking_ref  text,
    baggage      text,
    passenger    text
);

CREATE TABLE IF NOT EXISTS packing_items (
    id        uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id   uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    name      text NOT NULL,
    category  text NOT NULL DEFAULT 'Other'
        CHECK (category IN ('Clothing', 'Tech', 'Toiletries', 'Documents', 'Gear', 'Other')),
    is_packed boolean NOT NULL DEFAULT false,
    owner     text
);

CREATE TABLE IF NOT EXISTS expenses (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id      uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    title        text NOT NULL,
    amount       numeric NOT NULL,
    currency     text NOT NULL DEFAULT 'PHP',
    category     text NOT NULL DEFAULT 'Other'
        CHECK (category IN ('Food', 'Transport', 'Activity', 'Accommodation', 'Shopping', 'Other')),
    expense_date date NOT NULL,
    paid_by      text,
    photo_url    text,
    place_name   text,
    split_type   text CHECK (split_type IN ('Equal', 'Custom', 'Individual')),
    notes        text
);

CREATE TABLE IF NOT EXISTS places (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id         uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    name            text NOT NULL,
    category        text NOT NULL DEFAULT 'Do'
        CHECK (category IN ('Eat', 'Do', 'Nature', 'Essentials', 'Transport', 'Nightlife', 'Wellness', 'Culture', 'Coffee')),
    distance        text,
    notes           text,
    price_estimate  text,
    rating          integer,
    source          text NOT NULL DEFAULT 'Manual'
        CHECK (source IN ('Suggested', 'Manual', 'Friend Rec')),
    vote            text NOT NULL DEFAULT 'Pending'
        CHECK (vote IN ('👍 Yes', '👎 No', 'Pending')),
    photo_url       text,
    google_place_id text,
    google_maps_uri text,
    total_ratings   integer,
    latitude        numeric,
    longitude       numeric,
    saved           boolean NOT NULL DEFAULT true
);

CREATE TABLE IF NOT EXISTS checklist_items (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id    uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    title      text NOT NULL,
    is_done    boolean NOT NULL DEFAULT false,
    done_by    text,
    sort_order integer DEFAULT 0
);

CREATE TABLE IF NOT EXISTS moments (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id      uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    caption      text NOT NULL DEFAULT 'Untitled',
    storage_path text,
    public_url   text,
    location     text,
    uploaded_by  text,
    taken_at     date NOT NULL,
    tags         text[] DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS trip_files (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id       uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    name          text NOT NULL,
    file_url      text,
    file_type     text NOT NULL DEFAULT 'Other'
        CHECK (file_type IN ('Boarding Pass', 'Hotel Confirmation', 'Itinerary', 'Insurance', 'ID/Passport', 'Receipt', 'Other')),
    description   text,
    print_required boolean NOT NULL DEFAULT false
);

CREATE TABLE IF NOT EXISTS trip_insights (          -- inferred: not referenced in lib/supabase.ts
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id    uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    summary    text,
    news_items jsonb DEFAULT '[]'::jsonb,
    fetched_at timestamptz DEFAULT now(),
    expires_at timestamptz NOT NULL
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id       uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    sender_name   text NOT NULL,
    sender_avatar text,
    message       text NOT NULL,
    created_at    timestamptz DEFAULT now()
);

CREATE TABLE IF NOT EXISTS trip_invites (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trip_id    uuid NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    code       text NOT NULL UNIQUE,
    expires_at timestamptz,
    used       boolean NOT NULL DEFAULT false,
    created_at timestamptz DEFAULT now()
);

CREATE TABLE IF NOT EXISTS lifetime_stats (
    user_id            text PRIMARY KEY,
    total_trips        integer,
    total_countries    integer,
    total_nights       integer,
    total_miles        numeric,
    total_spent        numeric,
    home_currency      text,
    total_moments      integer,
    countries_list     text[],
    earliest_trip_date date
);

CREATE TABLE IF NOT EXISTS highlights (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         text,
    type            text,
    display_text    text,
    supporting_data jsonb,
    rank            integer
);

CREATE TABLE IF NOT EXISTS schema_migrations (
    version    text PRIMARY KEY,
    applied_at timestamptz DEFAULT now()
);

INSERT INTO schema_migrations (version) VALUES ('0001_init') ON CONFLICT DO NOTHING;

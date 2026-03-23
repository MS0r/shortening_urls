-- Add geo tracking to clicks
ALTER TABLE clicks ADD COLUMN country VARCHAR(2);
ALTER TABLE clicks ADD COLUMN city VARCHAR(100);
ALTER TABLE clicks ADD COLUMN ip_hash VARCHAR(64);

-- Add device tracking
ALTER TABLE clicks ADD COLUMN device_type VARCHAR(20);
ALTER TABLE clicks ADD COLUMN browser VARCHAR(50);
ALTER TABLE clicks ADD COLUMN os VARCHAR(50);

-- Create indexes for analytics
CREATE INDEX idx_clicks_country ON clicks(country);
CREATE INDEX idx_clicks_device_type ON clicks(device_type);
CREATE INDEX idx_clicks_clicked_at_hour ON clicks(clicked_at);

-- Add is_active column to urls if not exists (should exist from initial migration)
-- This is already present in 001_initial.sql

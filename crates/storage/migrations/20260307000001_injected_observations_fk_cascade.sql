-- Add foreign key constraint with CASCADE delete to injected_observations
ALTER TABLE injected_observations
ADD CONSTRAINT injected_observations_observation_id_fkey
FOREIGN KEY (observation_id) REFERENCES observations(id) ON DELETE CASCADE;

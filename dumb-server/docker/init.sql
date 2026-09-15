-- Schema source of truth. Runs once, only when the MySQL data volume is empty
-- (after a schema change: `docker compose -f docker/docker-compose.yml down -v`).

CREATE TABLE profiles (
  id           BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  display_name VARCHAR(16)  NOT NULL,
  pos_x        DOUBLE       NOT NULL DEFAULT 0,
  pos_z        DOUBLE       NOT NULL DEFAULT 0,
  yaw          DOUBLE       NOT NULL DEFAULT 0,
  updated_at   TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  KEY idx_profiles_display_name (display_name)
) ENGINE=InnoDB;

#!/bin/bash

cd backend || true

# Exit immediately if a command exits with a non-zero status
set -e

# Step 0: Install systemctl
if command -v systemctl >/dev/null 2>&1; then
    echo "systemctl is already installed at $(command -v systemctl)"
else
    echo "Installing systemctl..."
    sudo apt update
    sudo apt install -y systemd
fi

# Step 1: Create a Python HTTP server systemd service
echo "Creating Python HTTP server systemd service..."

# Define the service file path
SERVICE_FILE="/etc/systemd/system/simple_python_http.service"

# Current pwd
PWD=$(pwd)

# Write the service file
sudo tee $SERVICE_FILE > /dev/null <<EOF
[Unit]
Description=Simple Python HTTP Server
After=network.target

[Service]
ExecStart=/usr/bin/sh -c "sleep 5 && /usr/bin/python3 -m http.server 8000"
WorkingDirectory=$PWD
Restart=always
User=$(whoami)

[Install]
WantedBy=multi-user.target
EOF

# Reload systemd to recognize the new service
sudo systemctl daemon-reload

# Start and enable the service to run on boot
sudo systemctl start simple_python_http

echo "Python HTTP server service created and started."

# Step 2: Install NGINX
echo "Installing NGINX..."
sudo apt update -y || true
sudo apt install -y nginx

# Step 3: Configure NGINX as a reverse proxy
echo "Configuring NGINX as a reverse proxy to the Python HTTP server..."

# Define the NGINX configuration file
NGINX_CONFIG="/etc/nginx/sites-available/python_http_proxy"

# Write the NGINX config for proxying to the Python HTTP server
sudo tee $NGINX_CONFIG > /dev/null <<EOF
server {
    listen 80;

    access_log /var/log/nginx/python_http_proxy_access.log;

    location / {
        proxy_pass http://localhost:8000;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
EOF

# Enable the NGINX configuration by creating a symlink to sites-enabled
sudo ln -fs $NGINX_CONFIG /etc/nginx/sites-enabled/

# Define the NGINX configuration file
NGINX_HIBERNATOR_CONFIG="/etc/nginx/sites-available/hibernator"

# Write the NGINX config for proxying to the Python HTTP server
sudo tee $NGINX_HIBERNATOR_CONFIG > /dev/null <<EOF
server {
    listen 80;

    location /hibernator-landing/ {
        alias $PWD/../landing/;
    }

    location / {
        proxy_pass http://localhost:7878;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
EOF

# Remove the default NGINX configuration
sudo rm /etc/nginx/sites-enabled/default || true

# Test NGINX configuration for syntax errors
sudo nginx -t

# Restart NGINX to apply the new configuration
sudo nginx -s reload || sudo systemctl restart nginx

echo "NGINX is configured to proxy traffic to the Python HTTP server."

# Step 4: Ensure www-data can access the landing directory using ACLs
echo "Ensuring www-data can access landing directory..."

sudo apt install -y acl

LANDING_PATH="$(realpath "$PWD/../landing")"
USER="www-data"

if [ ! -d "$LANDING_PATH" ]; then
  echo "❌ Error: Landing directory not found at $LANDING_PATH"
  exit 1
fi

PARENTS=()
DIR="$LANDING_PATH"
while [ "$DIR" != "/" ]; do
  PARENTS+=("$DIR")
  DIR="$(dirname "$DIR")"
done

PARENTS=( $(printf "%s\n" "${PARENTS[@]}" | tac) )
for DIR in "${PARENTS[@]:0:${#PARENTS[@]}-1}"; do
  sudo setfacl -m "u:${USER}:x" "$DIR" 2>/dev/null || true
done

sudo setfacl -R -m "u:${USER}:rx" "$LANDING_PATH"

echo "✅ ACL permissions successfully granted for ${USER} on ${LANDING_PATH}"

# Final message
echo "Setup complete. Python HTTP server is running on port 8000, and NGINX is proxying traffic from port 80."

#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

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
ExecStart=/usr/bin/python3 -m http.server 8000
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
sudo nginx -s reload

echo "NGINX is configured to proxy traffic to the Python HTTP server."

# Final message
echo "Setup complete. Python HTTP server is running on port 8000, and NGINX is proxying traffic from port 80."

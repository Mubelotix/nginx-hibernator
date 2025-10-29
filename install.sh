#!/bin/sh

green='\033[0;32m'
red='\033[1;31m'
normal='\033[0m'

# This script uses the Rustup script as a library to detect the architecture of the system
source_rustup_functions() {
    echo "[1/7] Downloading library"
    rust_script=$(curl --proto '=https' --tlsv1.2 -sS https://sh.rustup.rs)
    if [ $? -ne 0 ]; then
        echo "${red}Error: Failed to download library${normal}"
        return 1
    fi

    line_number=$(echo "$rust_script" | grep -n "set +u" | cut -d: -f1)
    if [ -z "$line_number" ]; then
        echo "${red}Error: An update to the Rustup script has broken this script.${normal} Please open an issue at https://github.com/Mubelotix/nginx-hibernator/issues"
        return 1
    fi

    rust_script=$(echo "$rust_script" | head -n $line_number)
    eval "$rust_script"
}

source_rustup_functions

get_architecture
arch="$RETVAL"
filename="nginx-hibernator_${arch}"
latest_url=$(curl -sSL -w "%{url_effective}" -o /dev/null "https://github.com/Mubelotix/nginx-hibernator/releases/latest")
version=$(echo "$latest_url" | sed 's:.*/::')
download_url="https://github.com/mubelotix/nginx-hibernator/releases/download/$version/nginx-hibernator_$arch"

echo "[2/7] Downloading nginx-hibernator backend $version"
curl --fail --location --progress-bar "$download_url" -o "/tmp/$filename"
case $? in
    0)  ;;
    22) echo "${red}No available backend binary for your system ($arch).${normal} Please build from source: https://github.com/Mubelotix/nginx-hibernator"; exit 1 ;;
    *)  echo "${red}Error: Failed to download nginx-hibernator backend${normal}"; exit 1 ;;
esac

echo "[3/7] Downloading nginx-hibernator frontend $version"
download_url="https://github.com/mubelotix/nginx-hibernator/releases/download/$version/frontend.zip"
curl --fail --location --progress-bar "$download_url" -o "/tmp/frontend.zip"
case $? in
    0)  ;;
    22) echo "${red}Frontend not found for version $version.${normal} Please build from source: https://github.com/Mubelotix/nginx-hibernator"; exit 1 ;;
    *)  echo "${red}Error: Failed to download nginx-hibernator frontend${normal}"; exit 1 ;;
esac

echo "[4/7] Downloading nginx-hibernator default landing page $version"
download_url="https://github.com/mubelotix/nginx-hibernator/releases/download/$version/landing.zip"
curl --fail --location --progress-bar "$download_url" -o "/tmp/landing.zip"
case $? in
    0)  ;;
    22) echo "${red}Landing page not found for version $version.${normal} Please build from source: https://github.com/Mubelotix/nginx-hibernator"; exit 1 ;;
    *)  echo "${red}Error: Failed to download nginx-hibernator landing page${normal}"; exit 1 ;;
esac

echo "[5/7] Installing nginx-hibernator at /usr/local/bin/nginx-hibernator"
sudo mv "/tmp/$filename" "/usr/local/bin/nginx-hibernator"
chmod +x "/usr/local/bin/nginx-hibernator"

echo "[6/7] Installing nginx-hibernator frontend at /usr/share/nginx/html/nginx-hibernator-frontend"
sudo unzip -o "/tmp/frontend.zip" -d "/usr/share/nginx/html/nginx-hibernator-frontend"
sudo chown -R www-data:www-data "/usr/share/nginx/html/nginx-hibernator-frontend"
sudo chmod -R 755 "/usr/share/nginx/html/nginx-hibernator-frontend"

echo "[7/7] Installing nginx-hibernator default landing page at /usr/share/nginx/html/nginx-hibernator-landing"
sudo unzip -o "/tmp/landing.zip" -d "/usr/share/nginx/html/nginx-hibernator-landing"
sudo chown -R www-data:www-data "/usr/share/nginx/html/nginx-hibernator-landing"
sudo chmod -R 755 "/usr/share/nginx/html/nginx-hibernator-landing"

echo "${green}nginx-hibernator $version has been installed successfully!${normal}"

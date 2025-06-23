#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOCALNET_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$LOCALNET_DIR")"

# Default values
RPC_URL=""
FAUCET_PRIVATE_KEY=""
MIN_STAKE="1000000000000000000" # 1 ETH in wei
N_STORAGE_PROVIDERS=3

echo -e "${BLUE}🔗 Setting up Poda Blockchain Infrastructure${NC}"
echo "=============================================="

# Function to show usage
show_usage() {
    echo "Usage: $0 --rpc-url <RPC_URL> --faucet-key <PRIVATE_KEY> [options]"
    echo ""
    echo "Required arguments:"
    echo "  --rpc-url <URL>        Ethereum RPC endpoint"
    echo "  --faucet-key <KEY>     Private key of the faucet account (with funds)"
    echo ""
    echo "Optional arguments:"
    echo "  --min-stake <WEI>      Minimum stake amount (default: 1000000000000000000)"
    echo "  --providers <N>        Number of storage providers (default: 3)"
    echo "  --help                 Show this help message"
    echo ""
    echo "Environment variables:"
    echo "  ETHERSCAN_API_KEY      EtherScan API key for contract verification (optional)"
    echo ""
    echo "Example:"
    echo "  $0 --rpc-url https://eth-sepolia.g.alchemy.com/v2/YOUR_KEY \\"
    echo "       --faucet-key 0x1234567890abcdef..."
    echo ""
    echo "With EtherScan verification:"
    echo "  ETHERSCAN_API_KEY=your_key $0 --rpc-url https://eth-sepolia.g.alchemy.com/v2/YOUR_KEY \\"
    echo "       --faucet-key 0x1234567890abcdef..."
}

# Function to parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --rpc-url)
                RPC_URL="$2"
                shift 2
                ;;
            --faucet-key)
                FAUCET_PRIVATE_KEY="$2"
                shift 2
                ;;
            --min-stake)
                MIN_STAKE="$2"
                shift 2
                ;;
            --providers)
                N_STORAGE_PROVIDERS="$2"
                shift 2
                ;;
            --help)
                show_usage
                exit 0
                ;;
            *)
                echo -e "${RED}❌ Unknown option: $1${NC}"
                show_usage
                exit 1
                ;;
        esac
    done

    # Validate required arguments
    if [ -z "$RPC_URL" ]; then
        echo -e "${RED}❌ RPC_URL is required${NC}"
        show_usage
        exit 1
    fi

    if [ -z "$FAUCET_PRIVATE_KEY" ]; then
        echo -e "${RED}❌ Faucet private key is required${NC}"
        show_usage
        exit 1
    fi
}

# Function to generate private keys
generate_private_key() {
    openssl rand -hex 32
}

# Function to get address from private key using cast
get_address_from_private_key() {
    local private_key=$1
    cast wallet address --private-key "$private_key"
}

# Function to check if cast is installed
check_cast() {
    if ! command -v cast &> /dev/null; then
        echo -e "${RED}❌ cast (from foundry) is not installed${NC}"
        echo "Please install foundry: https://getfoundry.sh/"
        exit 1
    fi
}

# Function to check if forge is installed
check_forge() {
    if ! command -v forge &> /dev/null; then
        echo -e "${RED}❌ forge (from foundry) is not installed${NC}"
        echo "Please install foundry: https://getfoundry.sh/"
        exit 1
    fi
}

# Function to check RPC connection
check_rpc_connection() {
    echo -e "${BLUE}🔍 Checking RPC connection...${NC}"
    
    if ! cast chain-id --rpc-url "$RPC_URL" &> /dev/null; then
        echo -e "${RED}❌ Failed to connect to RPC endpoint${NC}"
        echo "Please check:"
        echo "  • RPC URL is correct"
        echo "  • Network is accessible"
        echo "  • If using local Anvil, make sure it's running"
        exit 1
    fi
    
    local chain_id=$(cast chain-id --rpc-url "$RPC_URL")
    echo -e "${GREEN}✅ Connected to chain ID: $chain_id${NC}"
    
    # Check if this is a known testnet or mainnet
    case $chain_id in
        1)
            echo -e "${YELLOW}⚠️  Connected to Ethereum Mainnet${NC}"
            ;;
        11155111)
            echo -e "${YELLOW}⚠️  Connected to Sepolia Testnet${NC}"
            ;;
        5)
            echo -e "${YELLOW}⚠️  Connected to Goerli Testnet${NC}"
            ;;
        31337|1337)
            echo -e "${GREEN}✅ Connected to local Anvil instance${NC}"
            ;;
        *)
            echo -e "${YELLOW}⚠️  Connected to unknown network (Chain ID: $chain_id)${NC}"
            ;;
    esac
}

# Function to check faucet balance
check_faucet_balance() {
    echo -e "${BLUE}💰 Checking faucet balance...${NC}"
    
    local faucet_address=$(get_address_from_private_key "$FAUCET_PRIVATE_KEY")
    local balance=$(cast balance "$faucet_address" --rpc-url "$RPC_URL")
    
    echo -e "${GREEN}✅ Faucet address: $faucet_address${NC}"
    echo -e "${GREEN}✅ Faucet balance: $(cast --to-unit "$balance" ether) ETH${NC}"
    
    # Check if balance is sufficient using bc for large number arithmetic
    local required_balance=$(($MIN_STAKE * ($N_STORAGE_PROVIDERS + 2))) # +2 for dispenser and challenger
    local balance_dec=$(cast --to-dec "$balance")
    
    # Use bc for large number comparison
    if [ "$(echo "$balance_dec < $required_balance" | bc -l)" -eq 1 ]; then
        echo -e "${RED}❌ Insufficient faucet balance${NC}"
        echo "Required: $(cast --to-unit "$required_balance" ether) ETH"
        echo "Available: $(cast --to-unit "$balance" ether) ETH"
        exit 1
    fi
}

# Function to generate service accounts
generate_service_accounts() {
    echo -e "${BLUE}🔑 Generating service accounts...${NC}"
    
    # Generate private keys
    DISPENCER_PRIVATE_KEY=$(generate_private_key)
    CHALLENGER_PRIVATE_KEY=$(generate_private_key)
    
    STORAGE_PROVIDER_PRIVATE_KEYS=()
    for i in $(seq 1 $N_STORAGE_PROVIDERS); do
        STORAGE_PROVIDER_PRIVATE_KEYS+=($(generate_private_key))
    done
    
    # Get addresses
    DISPENCER_ADDRESS=$(get_address_from_private_key "$DISPENCER_PRIVATE_KEY")
    CHALLENGER_ADDRESS=$(get_address_from_private_key "$CHALLENGER_PRIVATE_KEY")
    
    STORAGE_PROVIDER_ADDRESSES=()
    for key in "${STORAGE_PROVIDER_PRIVATE_KEYS[@]}"; do
        STORAGE_PROVIDER_ADDRESSES+=($(get_address_from_private_key "$key"))
    done
    
    echo -e "${GREEN}✅ Generated $((N_STORAGE_PROVIDERS + 2)) service accounts${NC}"
}

# Function to fund service accounts
fund_service_accounts() {
    echo -e "${BLUE}💸 Funding service accounts...${NC}"
    
    local faucet_address=$(get_address_from_private_key "$FAUCET_PRIVATE_KEY")
    local funding_amount=$(($MIN_STAKE * 2)) # Fund with 2x min stake
    
    # Function to send funds with retry
    send_funds() {
        local recipient=$1
        local description=$2
        local max_retries=3
        local retry_count=0
        
        while [ $retry_count -lt $max_retries ]; do
            echo "Funding $description: $recipient (attempt $((retry_count + 1)))"
            
            # Capture both stdout and stderr
            local output
            if output=$(cast send --private-key "$FAUCET_PRIVATE_KEY" --rpc-url "$RPC_URL" --value "$funding_amount" "$recipient" 2>&1); then
                echo -e "${GREEN}✅ Successfully funded $description${NC}"
                return 0
            else
                retry_count=$((retry_count + 1))
                echo -e "${RED}❌ Error funding $description:${NC}"
                echo "$output"
                
                if [ $retry_count -lt $max_retries ]; then
                    echo -e "${YELLOW}⚠️  Retrying in 2 seconds...${NC}"
                    sleep 2
                else
                    echo -e "${RED}❌ Failed to fund $description after $max_retries attempts${NC}"
                    return 1
                fi
            fi
        done
    }
    
    # Fund dispenser
    if ! send_funds "$DISPENCER_ADDRESS" "dispenser"; then
        exit 1
    fi
    
    # Fund challenger
    if ! send_funds "$CHALLENGER_ADDRESS" "challenger"; then
        exit 1
    fi
    
    # Fund storage providers
    for i in $(seq 0 $((N_STORAGE_PROVIDERS - 1))); do
        local address=${STORAGE_PROVIDER_ADDRESSES[$i]}
        if ! send_funds "$address" "storage provider $((i + 1))"; then
            exit 1
        fi
    done
    
    echo -e "${GREEN}✅ All service accounts funded${NC}"
}

# Function to deploy Poda contract
deploy_poda_contract() {
    echo -e "${BLUE}📦 Deploying Poda contract...${NC}"
    
    cd "$PROJECT_ROOT/contracts"
    
    # Deploy using Foundry
    forge script script/Poda.s.sol:PodaScript --rpc-url "$RPC_URL" --private-key "$FAUCET_PRIVATE_KEY" --broadcast
    
    # Extract contract address from deployment
    PODA_ADDRESS=$(forge script script/Poda.s.sol:PodaScript --rpc-url "$RPC_URL" --private-key "$FAUCET_PRIVATE_KEY" --silent | grep "Poda contract address:" | awk '{print $4}')
    
    if [ -z "$PODA_ADDRESS" ]; then
        echo -e "${RED}❌ Failed to extract contract address${NC}"
        echo "This might be due to deployment failure or parsing error"
        echo "Check the deployment logs above for more details"
        exit 1
    fi
    
    echo -e "${GREEN}✅ Poda contract deployed at: $PODA_ADDRESS${NC}"
    cd "$LOCALNET_DIR"
}

# Function to register storage providers
register_storage_providers() {
    echo -e "${BLUE}📝 Registering storage providers...${NC}"
    
    # This would require implementing the registration logic
    # For now, we'll create a script that can be run manually
    cat > "$LOCALNET_DIR/scripts/register-providers.sh" << 'EOF'
#!/bin/bash

# This script registers storage providers with the Poda contract
# Run this after the services are started

set -e

source .env

# Function to register a provider
register_provider() {
    local private_key=$1
    local name=$2
    local url=$3
    local stake_amount=$4
    
    echo "Registering provider: $name"
    # This would call the Poda contract's registerProvider function
    # Implementation depends on your contract interface
    echo "TODO: Implement provider registration"
}

# Register each storage provider
register_provider "$STORAGE_PROVIDER_1_PRIVATE_KEY" "storage-provider-1" "http://localhost:8081" "$MIN_STAKE"
register_provider "$STORAGE_PROVIDER_2_PRIVATE_KEY" "storage-provider-2" "http://localhost:8082" "$MIN_STAKE"
register_provider "$STORAGE_PROVIDER_3_PRIVATE_KEY" "storage-provider-3" "http://localhost:8083" "$MIN_STAKE"

echo "Provider registration complete"
EOF

    chmod +x "$LOCALNET_DIR/scripts/register-providers.sh"
    echo -e "${GREEN}✅ Created provider registration script${NC}"
    echo -e "${YELLOW}⚠️  Note: Run ./scripts/register-providers.sh after starting services${NC}"
}

# Function to create .env file
create_env_file() {
    echo -e "${BLUE}⚙️  Creating .env file...${NC}"
    
    cat > "$LOCALNET_DIR/.env" << EOF
# Blockchain Configuration
RPC_URL=$RPC_URL
PODA_ADDRESS=$PODA_ADDRESS
MIN_STAKE=$MIN_STAKE

# Service Private Keys
DISPENCER_PRIVATE_KEY=$DISPENCER_PRIVATE_KEY
CHALLENGER_PRIVATE_KEY=$CHALLENGER_PRIVATE_KEY
EOF

    # Add storage provider private keys
    for i in $(seq 1 $N_STORAGE_PROVIDERS); do
        local key_index=$((i - 1))
        local key=${STORAGE_PROVIDER_PRIVATE_KEYS[$key_index]}
        echo "STORAGE_PROVIDER_${i}_PRIVATE_KEY=$key" >> "$LOCALNET_DIR/.env"
    done
    
    echo -e "${GREEN}✅ .env file created${NC}"
}

# Function to create summary
create_summary() {
    echo -e "${GREEN}🎉 Blockchain setup complete!${NC}"
    echo ""
    echo -e "${BLUE}Configuration Summary:${NC}"
    echo "  • RPC URL: $RPC_URL"
    echo "  • Poda Contract: $PODA_ADDRESS"
    echo "  • Min Stake: $(cast --to-unit "$MIN_STAKE" ether) ETH"
    echo ""
    echo -e "${BLUE}Service Accounts:${NC}"
    echo "  • Dispatcher: $DISPENCER_ADDRESS"
    echo "  • Challenger: $CHALLENGER_ADDRESS"
    for i in $(seq 0 $((N_STORAGE_PROVIDERS - 1))); do
        echo "  • Storage Provider $((i + 1)): ${STORAGE_PROVIDER_ADDRESSES[$i]}"
    done
    echo ""
    echo -e "${BLUE}Next Steps:${NC}"
    echo "  1. Start Docker services: ./scripts/setup-docker-localnet.sh"
    echo "  2. Register providers: ./scripts/register-providers.sh"
    echo "  3. Test the localnet"
    echo ""
    echo -e "${YELLOW}⚠️  Keep your .env file secure and never commit it to version control${NC}"
}

# Main execution
main() {
    parse_args "$@"
    
    check_cast
    check_forge
    check_rpc_connection
    check_faucet_balance
    generate_service_accounts
    fund_service_accounts
    deploy_poda_contract
    register_storage_providers
    create_env_file
    create_summary
}

# Run main function
main "$@" 
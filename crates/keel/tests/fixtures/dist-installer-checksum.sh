# Extracted verbatim from the shell installer `dist` 0.32.0 generates for this
# workspace — `dist build --artifacts=global`, 2026-08-14. Four functions and
# one block out of `install()`, not the whole 50 KB script, because these are
# the ones the checksum path is made of and the rest is download and unpack.
#
# The block from `install()` was added on 2026-08-15: it is the guard that
# decides whether `verify_checksum` is called at all, and the branch Keel 0.1.2
# shipped down (KEEL-228). Its indentation is the installer's, because
# `scripts/patch-installer.sh` matches literally.
#
# It is checked in so `scripts/patch-installer.sh` has something to be tested
# against without `dist` on the machine. It is a *copy*, so it can go stale —
# which is why the patch script fails loudly on text it does not recognise
# rather than trusting this file to still be current. If a `dist` upgrade
# changes the block, the release fails and this fixture is regenerated.
#
# `PRINT_QUIET` is set here; in the real installer it is a flag.
PRINT_QUIET=0

say() {
    if [ "0" = "$PRINT_QUIET" ]; then
        echo "$1"
    fi
}

err() {
    if [ "0" = "$PRINT_QUIET" ]; then
        local red
        local reset
        red=$(tput setaf 1 2>/dev/null || echo '')
        reset=$(tput sgr0 2>/dev/null || echo '')
        say "${red}ERROR${reset}: $1" >&2
    fi
    exit 1
}

check_cmd() {
    command -v "$1" > /dev/null 2>&1
    return $?
}

verify_checksum() {
    local _file="$1"
    local _checksum_style="$2"
    local _checksum_value="$3"
    local _calculated_checksum

    if [ -z "$_checksum_value" ]; then
        return 0
    fi
    case "$_checksum_style" in
        sha256)
            if ! check_cmd sha256sum; then
                say "skipping sha256 checksum verification (it requires the 'sha256sum' command)"
                return 0
            fi
            _calculated_checksum="$(sha256sum -b "$_file" | awk '{printf $1}')"
            ;;
        sha512)
            if ! check_cmd sha512sum; then
                say "skipping sha512 checksum verification (it requires the 'sha512sum' command)"
                return 0
            fi
            _calculated_checksum="$(sha512sum -b "$_file" | awk '{printf $1}')"
            ;;
        sha3-256)
            if ! check_cmd openssl; then
                say "skipping sha3-256 checksum verification (it requires the 'openssl' command)"
                return 0
            fi
            _calculated_checksum="$(openssl dgst -sha3-256 "$_file" | awk '{printf $NF}')"
            ;;
        sha3-512)
            if ! check_cmd openssl; then
                say "skipping sha3-512 checksum verification (it requires the 'openssl' command)"
                return 0
            fi
            _calculated_checksum="$(openssl dgst -sha3-512 "$_file" | awk '{printf $NF}')"
            ;;
        blake2s)
            if ! check_cmd b2sum; then
                say "skipping blake2s checksum verification (it requires the 'b2sum' command)"
                return 0
            fi
            # Test if we have official b2sum with blake2s support
            local _well_known_blake2s_checksum="93314a61f470985a40f8da62df10ba0546dc5216e1d45847bf1dbaa42a0e97af"
            local _test_blake2s
            _test_blake2s="$(printf "can do blake2s" | b2sum -a blake2s | awk '{printf $1}')" || _test_blake2s=""

            if [ "X$_test_blake2s" = "X$_well_known_blake2s_checksum" ]; then
                _calculated_checksum="$(b2sum -a blake2s "$_file" | awk '{printf $1}')" || _calculated_checksum=""
            else
                say "skipping blake2s checksum verification (installed b2sum doesn't support blake2s)"
                return 0
            fi
            ;;
        blake2b)
            if ! check_cmd b2sum; then
                say "skipping blake2b checksum verification (it requires the 'b2sum' command)"
                return 0
            fi
            _calculated_checksum="$(b2sum "$_file" | awk '{printf $1}')"
            ;;
        false)
            ;;
        *)
            say "skipping unknown checksum style: $_checksum_style"
            return 0
            ;;
    esac

    if [ "$_calculated_checksum" != "$_checksum_value" ]; then
        err "checksum mismatch
            want: $_checksum_value
            got:  $_calculated_checksum"
    fi
}

# The caller, out of `install()`'s download loop. Everything between the markers
# is the installer's own text at the installer's own indentation; the wrapper
# exists only to give the locals it reads somewhere to come from.
download_and_verify() {
    local _file="$1"
    local _artifact_name="$2"
    local _checksum_style="$3"
    local _checksum_value="$4"

    # --- verbatim ---
        if [ -n "${_checksum_style:-}" ]; then
            verify_checksum "$_file" "$_checksum_style" "$_checksum_value"
        else
            say "no checksums to verify" 1>&2
        fi
    # --- end verbatim ---
}

# The harness the tests drive.
#
#   sh fixture.sh <file> <digest>                  verify_checksum directly
#   sh fixture.sh <file> <digest> <name> [<style>] through the caller's guard,
#                                                  with no style at all when it
#                                                  is left off — which is the
#                                                  shape 0.1.2 shipped in
if [ "$#" -ge 3 ]; then
    download_and_verify "$1" "$3" "${4-}" "$2"
else
    verify_checksum "$1" sha256 "$2"
fi

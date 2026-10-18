# Terminal host context only; no account, credential or provider operations.
sessionsation_host_environment() {
  [[ ${HERDR_ENV:-} == 1 ]] || return 0
  if [[ ! ${HERDR_PANE_ID:-} =~ ^w[0-9A-Za-z]+:p[0-9A-Za-z]+$ ]]; then
    printf '%s\n' 'HERDR_HOST_CONTEXT: missing or invalid HERDR_PANE_ID; open a new HERDR terminal.' >&2
    return 1
  fi
  [[ -n ${SESSIONSATION_HERDR_SESSION:-} || -n ${HERDR_SESSION:-} ]] && return 0
  local config_root=${XDG_CONFIG_HOME:-${HOME:-}/.config}
  local actual_socket default_socket
  if [[ $config_root == /* && -n ${HERDR_SOCKET_PATH:-} ]] &&
      actual_socket=$(readlink -f -- "$HERDR_SOCKET_PATH") &&
      default_socket=$(readlink -f -- "$config_root/herdr/herdr.sock") &&
      [[ -S $actual_socket && $actual_socket == "$default_socket" ]]; then
    # HERDR 0.9.5 omits HERDR_SESSION only for its default server. Prove the
    # socket identity rather than guessing "default" for an unknown server.
    export SESSIONSATION_HERDR_SESSION=default
    return 0
  fi
  printf '%s\n' 'HERDR_HOST_CONTEXT: session is missing and socket is not the verified default server; launch from a terminal with explicit HERDR_SESSION.' >&2
  return 1
}

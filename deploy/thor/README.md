# Starstar on Thor (k3s)

These manifests run Starstar on the Thor k3s cluster, reachable only on the tailnet at
**https://starstar.tailb56d83.ts.net**. They are plain Kustomize, applied with `kubectl`. ArgoCD is not used.

| File | What it creates or does |
|---|---|
| `namespace.yaml` | Namespace `starstar`, Pod Security `restricted` (enforce, audit, warn) |
| `networkpolicy.yaml` | `starstar-default-deny`, `starstar-api`, `starstar-postgres` |
| `database.yaml` | CloudNativePG Cluster `starstar-pg` (PostgreSQL 17.11, 1 instance, `local-path` 10Gi, TLS-only) |
| `library-volume.yaml` | Retained 100Gi local PV and PVC `starstar-library` on Thor |
| `api.yaml` | ServiceAccount `starstar-api` (no token), Deployment `starstar-api` (1 replica, Recreate), Service `starstar:8080` |
| `ingress.yaml` | Tailscale Ingress `starstar` (TLS by the Tailscale operator, tailnet only) |
| `kustomization.yaml` | Namespace, common label and the **image digest** |
| `preflight.sh` | Fail-closed check before every apply. Not a Kubernetes resource. |
| `first-deploy.sh` | Fail-closed staged first deployment. |

## Rules

- **Always** pass `--context thor` to cluster-facing commands. `kubectl kustomize` is offline and does not use a context. The default kube context is a different cluster.
- **Always** run `deploy/thor/preflight.sh thor [verified-rollback-image|--first-deploy-retry]` and get `PREFLIGHT OK` before `apply -k`, on the first deploy and on every upgrade.
- **Never** use `kubectl apply --prune` on this directory.
- **Never** run `kubectl delete -k deploy/thor/`, and never delete the namespace or Cluster `starstar-pg`. The PVC belongs to the Cluster and `local-path` has reclaim policy `Delete`. Deleting the Cluster deletes the database. **There is no off-node backup** (see [Backups](#backups-not-yet-configured)).
- Do not paste the API logs into chats or tickets before the first admin exists. They contain the one-time setup code.

## Networking and trust model

- **Tailnet → proxy:** the Tailscale operator runs a proxy pod (`ts-starstar-*` in namespace `tailscale`). It terminates TLS for `starstar.tailb56d83.ts.net` and forwards HTTP to Service `starstar:8080`. It appends the client's tailnet address to `X-Forwarded-For`.
- **Proxy → API:** `STARSTAR_TRUSTED_PROXIES=10.42.0.0/16` (the k3s cluster CIDR) makes the API trust `X-Forwarded-For` from any peer in that range. The API then uses the rightmost XFF entry that is not itself in the range. A client-supplied XFF is to the left of the proxy's entry, so a tailnet client cannot choose its own rate-limit key.
- **Why that is acceptable, and its limits:** among **pods**, NetworkPolicy `starstar-api` admits port 8080 only from this Ingress's proxy pod. Two qualifications:
  - NetworkPolicies are **additive**. Any other policy that selects the API pod and allows ingress widens the set of pods that can send a trusted XFF. Do not add one without revisiting this variable.
  - Traffic from the node itself (kubelet, host-network pods on Thor) is not filtered by these policies, and it can arrive from the node's `cni0` address inside `10.42.0.0/16`. Anything with that access is already privileged on Thor.
- **API → database:** NetworkPolicy allows port 5432 only from the API pod. `PGSSLMODE=verify-full` plus `PGSSLROOTCERT` (from `starstar-pg-ca`) make the API verify the server certificate and host name. On the server, `pg_hba` rejects every non-TLS TCP connection.
- **API → printers:** egress only to `192.168.1.0/24` TCP port 80 (PrusaLink over HTTP). This includes any LAN HTTP service in that range, by design. Register printers by **IP address**. mDNS names (`*.local`) do not resolve inside pods. A printer on another subnet or port is blocked until you change `networkpolicy.yaml`.
- **Database → Kubernetes API:** the CNPG instance manager reaches `10.43.0.1:443` / `192.168.1.2:6443`. The CNPG operator pod reaches the instance on port 8000.
- The API pod has no Kubernetes API token and no RBAC.

## Deploy

Run all commands from the repository root. Keep the terminal output: it is the deploy record.

### Library host preparation

Prepare the library directory once as root on Thor before applying the manifests. Check `zfs list hatch1/services` first. If it is a ZFS dataset, create the child dataset:

```sh
zfs create -p -o mountpoint=/hatch1/services/starstar/library hatch1/services/starstar/library
```

If `hatch1/services` is a plain directory instead, use `mkdir -p /hatch1/services/starstar/library`. Then, in either case:

```sh
chown 10001:10001 /hatch1/services/starstar/library
chmod 0750 /hatch1/services/starstar/library
```

The API mounts the PVC at `/var/lib/starstar/library`. The PV has reclaim policy `Retain`: deleting the PVC does not delete its files. Backups are planned for phase 3 and are not yet configured.

### First deployment

Set the digest in `kustomization.yaml` from the CI publish record. From the repository root, run the entire staged sequence as one script and retain its output as the deploy record:

```sh
deploy/thor/first-deploy.sh thor
```

It runs the image preflight, applies namespace and policies, performs the full target-namespace server dry run, then applies and waits for the database and its generated secrets before applying the application. Any failed command stops the sequence. The server dry run checks admission, not Pod Security of subsequently created pods. On failure inspect namespace events and pods/jobs; see Troubleshooting. If the Deployment exists but has never had a Ready pod and there is no rollback record, pass `--first-deploy-retry` as the second argument to preflight (or `first-deploy.sh`) to explicitly select the first-deploy retry path. Confirm that history from the deploy records; preflight cannot establish it from current pods. To reset a failed first deployment, run `kubectl --context thor -n starstar delete deployment starstar-api`, then retry. Never delete the Cluster or PVC. If a Ready pod existed previously, use the upgrade path and pass its verified rollback image; do not treat a failed upgrade as a first deployment.

### 4. Post-deploy verification

Run [Post-deploy verification](#post-deploy-verification) P1–P7 now. **Do not start first-admin setup until P1–P6 pass.** Before the first admin exists, anyone who can reach the API can claim it, and the setup limiter is keyed by client IP. P2 proves that the key is the real tailnet client.

### 5. First admin

1. Read the setup code yourself. Do not paste it anywhere:

   ```sh
   kubectl --context thor -n starstar logs deploy/starstar-api | grep 'setup code'
   ```

2. Open `https://starstar.tailb56d83.ts.net/setup` from a tailnet device and create the admin.
3. Sign in, then add printers by IP (`192.168.1.x`). Use the printer's connection check as the positive printer test (P5).

The code is valid only until the first admin exists. It changes on every restart while there are no users.

## Post-deploy verification

Run these after every first deploy. After an upgrade that changes `networkpolicy.yaml`, run P1–P6; after changes to `ingress.yaml`, `database.yaml` or the TLS environment, run P1, P2 and P6. Record each result. Each block stops at its first failure.

Confirm positive controls for DB 5432 (API TLS session), 8000 (P4 operator status), and API 8080 (P1 health) before interpreting P3 denials. Define the helper and variables once (a function, because zsh does not word-split `$VAR`). The primary is resolved by label, never by a fixed pod name. Re-run this after any pod restart:

```sh
kss() { kubectl --context thor -n starstar "$@"; }
CLUSTER=${CLUSTER:-starstar-pg}
PG=$(kss get pod -l cnpg.io/cluster="$CLUSTER",cnpg.io/instanceRole=primary -o jsonpath='{.items[0].metadata.name}') \
  && API=$(kss get pod -l app.kubernetes.io/component=api -o jsonpath='{.items[0].metadata.name}') \
  && echo "primary=$PG api=$API"
```

### P1. Pod, startup log, Ingress, health

```sh
kss get pods -l app.kubernetes.io/component=api \
  && kss logs "$API" | grep -v 'setup code' \
  && kss get ingress starstar \
  && kubectl --context thor -n tailscale get pods -o wide \
       -l tailscale.com/parent-resource=starstar,tailscale.com/parent-resource-ns=starstar \
  && curl -fsS https://starstar.tailb56d83.ts.net/api/health && echo && echo "P1 OK"
```

Expect: pod `1/1 Running`, 0 restarts; the log shows `trusted proxies: 10.42.0.0/16` and `starstar api listening on 0.0.0.0:8080 (public URL: https://starstar.tailb56d83.ts.net, database: postgresql://starstar-pg-rw.starstar…)`, and no `plain HTTP` or `trusted proxies` warning; the Ingress ADDRESS is `starstar.tailb56d83.ts.net`; exactly one proxy pod with an IP in `10.42.0.0/16`; `curl` prints `{"status":"ok","app":"starstar",…}`. The `curl` also proves the Tailscale certificate is valid for the name (`curl` verifies it). Run the `curl` from a tailnet device.

### P2. Rate-limit key is the real tailnet client (behavioural)

The API does not log the rate-limit key, so check its behaviour. The login limiter allows 5 failures per (username, client IP) in 15 minutes. Use a throwaway username that will never exist, so no real account is affected. The per-username limit (20) is not reached.

From tailnet device **A**:

```sh
U=xffprobe$(date +%s); H=https://starstar.tailb56d83.ts.net/api/session
for i in 1 2 3 4 5; do
  curl -s -o /dev/null -w '%{http_code} ' -H 'content-type: application/json' \
    -H "X-Forwarded-For: 203.0.113.$i" -d "{\"username\":\"$U\",\"password\":\"wrong\"}" "$H"
done; echo
curl -s -o /dev/null -w '%{http_code}\n' -H 'content-type: application/json' \
  -H 'X-Forwarded-For: 198.51.100.7' -d "{\"username\":\"$U\",\"password\":\"wrong\"}" "$H"
echo "username for device B: $U"
```

Expect five `401`, then `429`. Each request forged a different `X-Forwarded-For`. If a forged value chose the key, the 6th request would get `401`. Then device A must also send the same username without XFF and receive `429`:

```sh
curl -s -o /dev/null -w '%{http_code}\n' -H 'content-type: application/json' -d "{\"username\":\"$U\",\"password\":\"wrong\"}" "$H"
```

From a **different** tailnet device **B**, within 15 minutes, with the same `U`:

```sh
curl -s -o /dev/null -w '%{http_code}\n' -H 'content-type: application/json' \
  -H 'X-Forwarded-For: 198.51.100.7' -d '{"username":"<U from device A>","password":"wrong"}' https://starstar.tailb56d83.ts.net/api/session
```

Expect `401`, not `429`: device B has its own key, so the proxy forwards the real client address. If only one tailnet device is available, P2 is incomplete: do not start first-admin setup until both devices pass.

The throwaway username's counter expires after 15 minutes. Nothing needs cleaning up.

### P3. NetworkPolicy: negative probes from a pod that is allowed to try

A probe whose own egress is blocked proves nothing. This probe gets a temporary egress policy that allows exactly the connections under test, so a `BLOCKED` result comes from the **target's** ingress policy. Delete both afterwards.

```sh
PGIP=$(kss get pod "$PG" -o jsonpath='{.status.podIP}') \
  && APIIP=$(kss get pod "$API" -o jsonpath='{.status.podIP}') \
  && echo "pg=$PGIP api=$APIIP"
```

```sh
kss apply -f - <<'EOF' &&
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: np-probe-egress
spec:
  podSelector:
    matchLabels:
      app.kubernetes.io/name: np-probe
  policyTypes: [Egress]
  egress:
    - to:
        - podSelector:
            matchLabels:
              cnpg.io/cluster: starstar-pg
      ports: [{port: 5432, protocol: TCP}, {port: 8000, protocol: TCP}]
    - to:
        - podSelector:
            matchLabels:
              app.kubernetes.io/component: api
      ports: [{port: 8080, protocol: TCP}]
    - to:
        - namespaceSelector:
            matchLabels:
              kubernetes.io/metadata.name: kube-system
          podSelector:
            matchLabels:
              k8s-app: kube-dns
      ports: [{port: 53, protocol: UDP}, {port: 53, protocol: TCP}]
EOF
kss run np-probe --rm -i --restart=Never --labels=app.kubernetes.io/name=np-probe \
  --image=docker.io/library/busybox:1.37 \
  --overrides='{"spec":{"securityContext":{"runAsNonRoot":true,"runAsUser":65534,"seccompProfile":{"type":"RuntimeDefault"}},"containers":[{"name":"np-probe","image":"docker.io/library/busybox:1.37","command":["sh","-c","nslookup '"$CLUSTER"'-rw.starstar.svc.cluster.local >/dev/null && echo DNS-OK || echo DNS-FAIL; for t in '"$PGIP"':5432 '"$PGIP"':8000 '"$APIIP"':8080; do nc -w 3 ${t%:*} ${t#*:} </dev/null && echo \"$t OPEN\" || echo \"$t BLOCKED\"; done"],"securityContext":{"allowPrivilegeEscalation":false,"capabilities":{"drop":["ALL"]}}}]}}'
probe_status=$?
kss delete networkpolicy np-probe-egress --ignore-not-found || probe_status=1
[ "$probe_status" -eq 0 ]
```

Expect `DNS-OK` (the probe's networking works), then `BLOCKED` for all three targets. Make sure `np-probe-egress` is gone: `kss get netpol` lists only the three `starstar-*` policies.

Negative test from a **nonmatching pod in `tailscale`** (the namespace the real proxy is in), for TCP/8080 to the API. First check that the namespace's policies, if any, let a new unlabelled pod make the attempt:

```sh
kubectl --context thor -n tailscale get networkpolicy
```

If that lists none, run the probe there. If it lists policies that select all pods, skip it and record why. The `starstar` probe above already covers the negative case.

```sh
kubectl --context thor -n tailscale run np-probe --rm -i --restart=Never --image=docker.io/library/busybox:1.37 \
  --overrides='{"spec":{"securityContext":{"runAsNonRoot":true,"runAsUser":65534,"seccompProfile":{"type":"RuntimeDefault"}},"containers":[{"name":"np-probe","image":"docker.io/library/busybox:1.37","command":["sh","-c","nc -w 3 '"$APIIP"' 8080 </dev/null && echo OPEN || echo BLOCKED"],"securityContext":{"allowPrivilegeEscalation":false,"capabilities":{"drop":["ALL"]}}}]}}'
```

Expect `BLOCKED`. The real proxy reaching the API is the positive case (P1 `curl`).

### P4. CNPG operator and instance paths

```sh
kss get cluster "$CLUSTER" -o jsonpath='{.status.phase}{"\n"}{.status.currentPrimary}{"\n"}{.status.instancesReportedState}{"\n"}' \
  && OPLOG=$(kubectl --context thor -n cnpg-system logs -l app.kubernetes.io/name=cloudnative-pg --since=15m) \
  && { if printf '%s\n' "$OPLOG" | grep '"namespace":"starstar"' | grep -iE 'refused|timeout|unreachable|i/o'; then echo 'operator errors found'; false; else echo 'no matching operator errors'; fi; }
```

Expect phase `Cluster in healthy state`, `currentPrimary` equal to `$PG`, `instancesReportedState` with `isPrimary: true` for it, and no connection errors from the operator about `starstar`. A reported state means the operator reached the instance manager on TCP/8000. A healthy instance with a recent status update means the instance manager reaches the Kubernetes API.

### P5. API egress: printer allowed, everything else blocked

Use a known listening printer on port 80, a known listening LAN endpoint on a non-80 port, and a reachable external TCP endpoint. From a separate permitted workstation or pod, first prove all three endpoints accept TCP connections; record addresses, ports and results. A refused or timed-out control makes that probe inconclusive. Set the variables below and run `nc -vz -w 3 "$PRINTER" 80`, `nc -vz -w 3 "$LAN_HOST" "$LAN_PORT"`, and `nc -vz -w 3 "$EXTERNAL_HOST" "$EXTERNAL_PORT"` from that permitted source. Then run the block from the API network namespace:

```sh
PRINTER="192.168.1.<printer>"
LAN_HOST="192.168.1.<listening-host>"
LAN_PORT="<listening-non-80-port>"
EXTERNAL_HOST="<reachable-external-ip>"
EXTERNAL_PORT="<listening-port>"
kss debug "$API" --profile=restricted --image=docker.io/library/busybox:1.37 -i --quiet -- \
  sh -c "nslookup kubernetes.default.svc.cluster.local; nc -w 3 $PRINTER 80 </dev/null && echo printer-OPEN || echo printer-FAILED; nc -w 3 $LAN_HOST $LAN_PORT </dev/null && echo lan-OPEN || echo lan-BLOCKED; nc -w 3 $EXTERNAL_HOST $EXTERNAL_PORT </dev/null && echo external-OPEN || echo external-BLOCKED"
```

Require DNS success, printer OPEN, and both controlled endpoints BLOCKED. Repeat the positive controls immediately if in doubt. The debug container remains terminated in the pod spec until replacement. Confirm DNS from the database workload too: `kss exec "$PG" -c postgres -- getent hosts kubernetes.default.svc.cluster.local`.

### P6. Database TLS: identity, not only encryption

Check the generated URI **without printing the password**. URL query parameters override `PGSSLMODE`/`PGSSLROOTCERT`, so the URI must have none that weaken them.

```sh
kss get secret "$CLUSTER-app" -o jsonpath='{.data.uri}' | CLUSTER="$CLUSTER" python3 -c '
import base64, os, sys
from urllib.parse import urlsplit, parse_qs
try:
    u = urlsplit(base64.b64decode(sys.stdin.buffer.read(), validate=True).decode())
    q = {k.lower() for k in parse_qs(u.query, keep_blank_values=True)}
    assert u.hostname == os.environ["CLUSTER"] + "-rw.starstar" and u.port == 5432
    assert not q.intersection({"sslmode", "sslrootcert"})
except Exception:
    sys.exit("URI host, port or TLS options invalid")
print("URI host, port and TLS options OK")'
```

The check prints no URI or credentials. It requires the selected cluster RW hostname, port 5432, and no `sslmode` or `sslrootcert` overrides. If it fails, inspect the URI privately and compare the host with the SANs below.

Check that the server certificate chains to the CA and contains the URI host:

```sh
(
set -o pipefail
D=$(mktemp -d) || exit 1
p6_result=0
kss get secret "$CLUSTER-ca" -o jsonpath='{.data.ca\.crt}' | base64 -d > "$D/ca.crt" \
  && kss get secret "$CLUSTER-server" -o jsonpath='{.data.tls\.crt}' | base64 -d > "$D/server.crt" \
  && openssl verify -CAfile "$D/ca.crt" -verify_hostname "${CLUSTER}-rw.starstar" "$D/server.crt" \
  && echo "P6 chain+SAN OK" || p6_result=$?
rm -rf "$D" || p6_result=1
exit "$p6_result"
)
```

These files are a CA certificate and a server certificate, not keys. Nothing secret is written.

Check that the API's live sessions use TLS (encryption only; the checks above and the API's successful `verify-full` start give identity):

```sh
kss exec "$PG" -c postgres -- psql -tAc \
  "select a.application_name, s.ssl, s.version from pg_stat_ssl s join pg_stat_activity a using (pid) where a.application_name = 'starstar-api'"
```

Expect one or more `starstar-api|t|TLSv1.3` lines.

Negative checks that the server identity is enforced. These use `psql` in the primary over loopback (`hostaddr=127.0.0.1`), so no policy change is needed. `host=` sets the name that is verified. No password is sent; a password error means the TLS checks passed.

```sh
kss exec -i "$PG" -c postgres -- sh -c '
  CA=$(mktemp 2>/dev/null || mktemp -p /controller); cat > "$CA"
  try() { PGCONNECT_TIMEOUT=5 psql "host=$1 hostaddr=127.0.0.1 dbname=starstar user=starstar sslmode=verify-full sslrootcert=$2" -c "select 1" 2>&1 </dev/null | head -1; }
  echo "right host, right CA:"; try "$1-rw.starstar" "$CA"
  echo "wrong host:";           try wrong.invalid "$CA"
  echo "wrong CA:";             try "$1-rw.starstar" /etc/ssl/certs/ca-certificates.crt
  rm -f "$CA"' sh "$CLUSTER" < <(kss get secret "$CLUSTER-ca" -o jsonpath='{.data.ca\.crt}' | base64 -d)
```

Expect: right host and CA → a password or authentication message (TLS succeeded); wrong host → `server certificate for "…" does not match host name "wrong.invalid"`; wrong CA → `certificate verify failed`. The CA file goes to `/tmp`, or to `/controller` if `/tmp` is read-only, and is removed. If `/etc/ssl/certs/ca-certificates.crt` does not exist in the image, use any other CA file that did not sign the server certificate.

### P7. Pod Security on CNPG-created pods

```sh
EVENTS=$(kss get events --sort-by=.lastTimestamp) && {
  if printf '%s\n' "$EVENTS" | grep -iE 'podsecurity|forbidden'; then
    echo 'P7 failed: matching events found' >&2; false
  else
    p7_result=$?
    [ "$p7_result" -eq 1 ] && echo 'P7 OK: no matching events'
  fi
}
```

Expect no matches. This covers the initdb Job and the instance, which the server dry run did not check.

## Upgrade

Every upgrade uses the same preflight as the first deploy. `strategy: Recreate` stops the old pod before the new one pulls its image, so a bad digest means an outage. The preflight prevents that.

1. Set the new digest in `kustomization.yaml`.
2. If the release adds a migration, take a [manual dump](#manual-dump) first.
3. Run the gate and the apply as one block. Keep the output: the `rollback image:` line is the pinned image from the Ready pod's spec. Preserve this verified record across retries. If there is no Ready pod, pass that recorded full image as the second preflight argument; never use the failed Deployment template as a rollback target.

   ```sh
   LOG="preflight-$(date +%Y%m%d-%H%M%S).log"
   # When no API pod is Ready, set ROLLBACK to the full verified image from the prior record.
   ROLLBACK="<verified-rollback-image-if-no-ready-pod>"
   deploy/thor/preflight.sh thor "$ROLLBACK" > "$LOG" && cat "$LOG" \
     && kubectl --context thor apply -k deploy/thor/ --dry-run=server \
     && kubectl --context thor apply -k deploy/thor/ \
     && kubectl --context thor -n starstar rollout status deploy/starstar-api --timeout=5m \
     && curl -fsS https://starstar.tailb56d83.ts.net/api/health && echo && echo "UPGRADE OK"
   ```

   The preflight writes its record to `$LOG` (no pipe, so a failure stops the chain); failures print on stderr.

Expect a short outage. In-flight printer uploads are cut at the 60 s grace period. Migrations run at startup under an advisory lock.

## Rollback

- **Image:** set the digest from the verified `rollback image:` line of the upgrade's preflight log, then run the same upgrade block. `kubectl --context thor -n starstar rollout undo deploy/starstar-api` also works, but the next `apply -k` reverts it, so fix `kustomization.yaml` as well.
- **Schema:** the API refuses to start if the database schema is newer than the binary. You cannot roll back past a release that added a migration. Without a backup, the only way back is a restore from the [manual dump](#manual-dump) taken before the upgrade.
- **Library migration v3:** once the new API has migrated Postgres to v3, pinning the previously verified pre-v3 image alone will leave the API unable to start because it rejects the newer schema. To return to that image, restore a verified pre-migration database dump using the procedure below; this loses database changes made since that dump. Keep a matching snapshot or copy of the library files when restoring a database from a later point, so its catalog and blobs correspond. The database dump does not contain library files.
- **Stop the app without touching data:** `kubectl --context thor -n starstar scale deploy/starstar-api --replicas=0`. The next `apply -k` sets it back to 1.
- **Undo a hardening change** (see Troubleshooting) by editing the manifest and applying again. Never delete and recreate the Cluster.

## Backups (not yet configured)

**What exists:** one copy of the database on the CNPG PVC under `/hatch1/k3s/storage/pvc-<uid>_starstar_starstar-pg-1`, and one copy of library files at `/hatch1/services/starstar/library`, on Thor's ZFS pool. There are no WAL archives (`archive_mode` is on, but nothing is configured to receive the WAL) and no `ScheduledBackup`.

**What is missing:** any off-node backup. Losing Thor's disk can lose the database and library; deleting the Cluster or namespace or a bad migration can lose database records. The manual database dump does not include library files.

### Manual dump

Take one before risky changes. Set `F` to a file **inside an already mounted encrypted workstation volume**, with sufficient space. The dump contains plaintext printer passwords. Use a fresh name; never overwrite a previous backup.

```sh
kss() { kubectl --context thor -n starstar "$@"; }
CLUSTER="starstar-pg"
F="<encrypted-mounted-directory>/starstar.dump"
PG=$(kss get pod -l cnpg.io/cluster="$CLUSTER",cnpg.io/instanceRole=primary -o jsonpath='{.items[0].metadata.name}') \
  && test -n "$PG" && test ! -e "$F" \
  && (umask 077; set -o noclobber; kss exec "$PG" -c postgres -- pg_dump -Fc -d starstar > "$F") \
  && kss exec "$PG" -c postgres -- createdb -O starstar starstar_dump_test \
  && kss exec -i "$PG" -c postgres -- pg_restore --no-owner --role=starstar --exit-on-error -d starstar_dump_test < "$F" \
  && kss exec "$PG" -c postgres -- psql -v ON_ERROR_STOP=1 -d starstar_dump_test -c 'select (select count(*) from users), (select count(*) from printers), (select count(*) from oauth_clients)' \
  && echo "test restore OK: $F"
```

Compare counts and application data with the source at the dump's point in time; concurrent writes can change counts. Only then consider the dump verified. Drop `starstar_dump_test` after inspection; on failure investigate and clean up explicitly. A restore reads the archive payload, unlike `pg_restore --list`. Keep the encrypted dump off-node.

### Restore a manual dump into a fresh database

This procedure restores into a fresh database in the existing `starstar-pg` Cluster. Choose a compatible image digest, set it in `kustomization.yaml`, and run preflight, preserving the rollback record. If no API pod is Ready, pass the full verified image from the previous rollback record: `deploy/thor/preflight.sh thor "$ROLLBACK"`, with `ROLLBACK` set to that image. Do not use the failed Deployment template as the record. Scale the API to zero and wait for all API pods to terminate. Keep it at zero during the restore. Resolve `PG` as above. Restore into a new `starstar_restore` database using the `createdb` and `pg_restore --exit-on-error` commands above, substituting `starstar_restore` and the selected encrypted dump. Compare counts and schema against the expected backup and check errors. Do not overwrite an existing recovery database.

With the API still stopped, restore a matching library snapshot or copy when the dump contains library catalog entries. A database dump alone cannot restore blobs or previews. Preserve the current library data for recovery and verify the restored files correspond to the catalog before restarting. With the API still stopped, terminate remaining connections and rename `starstar` to `starstar_old`, then `starstar_restore` to `starstar` using separate `ALTER DATABASE` commands as a PostgreSQL administrator. Retain `starstar_old` through verification.

Before applying the selected image, temporarily add the following to `kustomization.yaml` so the rendered Deployment remains stopped:

```yaml
replicas:
  - name: starstar-api
    count: 0
```

Confirm the chosen digest is in the `images` section, then render and apply it. `kubectl kustomize` is offline and does not use a context:

```sh
set -o pipefail
kubectl kustomize deploy/thor | kubectl --context thor apply -f - &&
kubectl --context thor -n starstar get deployment starstar-api -o jsonpath='{.spec.replicas}{" "}{.spec.template.spec.containers[?(@.name=="api")].image}{"\n"}'
```

Verify the output shows zero replicas and the chosen pinned image. Remove the temporary `replicas` entry from `kustomization.yaml` to restore the normal desired count, then scale to one, wait for rollout, and run P1–P7. Preserve the old database until verification succeeds. Off-node backup automation and cluster-replacement recovery are deferred follow-ups.

### How to add backups later

Follow the openbooks pattern, `openbooks/deploy/thor-k3s/instances/hatchbooks/backup-cronjob.yaml`:

- A CronJob `starstar-pg-backup` (`0 3 * * *`, `concurrencyPolicy: Forbid`).
  - An initContainer runs `pg_dump -Fc -h starstar-pg-rw -U starstar -d starstar` into a scratch PVC `starstar-backups` (`local-path`). `PGPASSWORD` comes from `starstar-pg-app` key `password`. Use `PGSSLMODE=verify-full` with `PGSSLROOTCERT` from `starstar-pg-ca`, as the API does.
  - A `restic/restic` container then runs `restic backup` to Backblaze B2 (`--host starstar --tag pg-backup`, `forget --keep-within 30d --prune`), with `envFrom` Secret `starstar-backup-b2` (`RESTIC_REPOSITORY`, `RESTIC_PASSWORD`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`).
- A SopsSecret for `starstar-backup-b2` (see below). Use a strong, separate `RESTIC_PASSWORD`. It is the only protection of the off-site printer passwords.
- NetworkPolicy changes:
  - backup pods (`app.kubernetes.io/component: backup`): egress to `starstar-pg` 5432, DNS, and public 443, excluding RFC 1918 and CGNAT;
  - `starstar-postgres`: ingress 5432 from those pods.
- A restore test before you call it done: `restic restore latest`, then the [fresh-database restore](#restore-a-manual-dump-into-a-fresh-database) above, and compare the row counts.

## Adding a secret

Nothing app-specific is needed today. The database URL and CA come from CNPG. The GHCR package is public. The setup code is random. To add one (for example `STARSTAR_SETUP_TOKEN`, a GHCR pull secret, or the B2 backup credentials):

1. Create `.sops.yaml` at the repository root, if it is not there:

   ```yaml
   creation_rules:
     - path_regex: deploy/thor/.*\.sops\.yaml$
       encrypted_regex: ^(data|stringData)$
       age: >-
         age1q0a7wkwqa2ngjwgrx2sjhmrm2p02an03ju9ruadatmxae0688yrqun8vf4,
         age1j5nffd9dr39gjtx940e8x0je9fvpv8uluzvwkv22rzk988sfey5quvr9hs
   ```

   The first recipient is the Thor cluster key, used by the in-cluster operator. The second is the workstation key.

2. Write `deploy/thor/secrets.sops.yaml`:

   ```yaml
   apiVersion: isindir.github.com/v1alpha3
   kind: SopsSecret
   metadata:
     name: starstar-secrets
     namespace: starstar
   spec:
     secretTemplates:
       - name: starstar-backup-b2
         stringData:
           RESTIC_REPOSITORY: "<value>"
           RESTIC_PASSWORD: "<value>"
           AWS_ACCESS_KEY_ID: "<value>"
           AWS_SECRET_ACCESS_KEY: "<value>"
   ```

3. Encrypt in place **before** `git add`: `sops --encrypt --in-place deploy/thor/secrets.sops.yaml`. Then check that `grep -c 'ENC\[' deploy/thor/secrets.sops.yaml` is non-zero and that no plaintext value remains.
4. Add `secrets.sops.yaml` to `resources:` in `kustomization.yaml`, **before** `database.yaml`, and reference the Secret from the workload.
5. Apply. The sops-secrets-operator (namespace `sops`) decrypts it in the cluster. Check with `kubectl --context thor -n starstar get sopssecret starstar-secrets` (names and status only).

Never commit a decrypted file. Never print decrypted values.

## Known follow-ups

- **Off-node backups and cluster-replacement recovery** are deferred follow-ups.
- **Printer passwords are stored in plaintext** in the `printers` table. Anyone with database access, or a dump, can read every PrusaLink password. Track an app change to encrypt them at rest.
- **Printer egress is the whole `192.168.1.0/24` on port 80.** That range includes Thor (`192.168.1.2`) and any LAN service on port 80. Narrow the range to the printers' `/32` addresses once they are fixed (DHCP reservations).
- **Large uploads** (up to 512 MB) through the Tailscale proxy are untested. Check them during the first real use.
- **Single-instance PDB:** `enablePDB: true` with one instance blocks `kubectl drain thor`. For node maintenance, use the CNPG maintenance window (`spec.nodeMaintenanceWindow: {inProgress: true, reusePVC: true}`) or accept downtime.
- **Volume size:** `local-path` cannot expand volumes. Raising `storage.size` in `database.yaml` does not grow the existing PVC. Growing it requires a replacement Cluster and restore; that recovery procedure is a deferred follow-up and is not yet documented here.

## Troubleshooting

- **API exits at startup with a TLS or certificate error** (`UnknownIssuer`, `NotValidForName`, `invalid peer certificate`). Keep `verify-full`. Diagnose first, using the P6 commands:
  1. **URI host:** print the redacted URI. Is its host `starstar-pg-rw.starstar`? Is there an `sslmode`/`sslrootcert` query parameter that overrides the environment?
  2. **SAN:** is the URI host in the server certificate's `subjectAltName`? `NotValidForName` means it is not.
  3. **CA:** does `openssl verify -CAfile ca.crt server.crt` pass? Does `PGSSLROOTCERT` in `api.yaml` reference `starstar-pg-ca` key `ca.crt`? `UnknownIssuer` means the chain does not reach that CA.
  4. Fix the cause (for example the Secret reference or the host the URI uses). Relaxing to `PGSSLMODE=require` gives up server identity checks. Do that only as a deliberate, recorded decision approved by the owner, never as the first response.
- **API logs `no pg_hba.conf entry … no encryption`:** the client is not using TLS. Check that `PGSSLMODE` is set and that the URI has no `sslmode=disable`.
- **The CNPG webhook or server dry run rejects `database.yaml`** (admission): read the message. If it names the `pg_hba` line, record it and decide explicitly; do not drop the line silently.
- **The Cluster never gets ready** (runtime bootstrap): this is a different problem from admission. Find which connection fails before you change anything:
  1. `kubectl --context thor -n starstar logs "<instance-pod-or-initdb-job-pod>" | grep -iE 'pg_hba|reject|refused|ssl'`.
  2. A line `pg_hba.conf rejects connection for host "…", user "…", database "…", no encryption` shows the exact client. If it is the operator's own connection, removing `hostnossl all all all reject` may be needed. Make that change explicitly, record it as a follow-up, and report it. Do not remove the rule for an unexplained failure.
  3. Refused or timed-out connections on 8000 or to the API server point at NetworkPolicy (P4), not at `pg_hba`.
- **Pods rejected with `violates PodSecurity "restricted"`:**
  1. Read the event to see which field is wrong. Fix the manifest if it is ours.
  2. If it is a CNPG pod, set `pod-security.kubernetes.io/enforce: baseline` in `namespace.yaml` (keep `warn`/`audit` restricted), apply, and record the follow-up.
- **Migration slow or timing out:** `STARSTAR_DB_CONNECT_TIMEOUT_SECS=60` bounds the connection, its retries **and** the migrations together. The 90 s startup probe does not extend that deadline. A long migration needs a larger value in `api.yaml`. The TCP liveness probe checks only that the listener accepts connections, and it starts only after the startup probe passes.
- **Proxy cannot reach the API (502 from the tailnet URL):**
  1. Compare the proxy pod labels (`kubectl --context thor -n tailscale get pods --show-labels | grep starstar`) with the `from` selector in NetworkPolicy `starstar-api`.
  2. If you renamed the Ingress, update `tailscale.com/parent-resource`.
- **Printer unreachable:**
  1. Check that the printer's IP is in `192.168.1.0/24` and that PrusaLink is on port 80.
  2. Check that the printer was registered by IP. `.local` names do not resolve in pods.
- **P2 fails: device B gets `429` after A exhausts its limit** (clients may share a key):
  1. Check that the startup log shows `trusted proxies: 10.42.0.0/16`.
  2. Check that the proxy pod IP is in `10.42.0.0/16`: `kubectl --context thor -n tailscale get pods -o wide | grep starstar`.
  3. The proxy may not send `X-Forwarded-For`, or may send `ip:port` (an unparsable entry makes the API use the peer). Do not start first-admin setup until this is fixed.
- **P2 fails: the 6th request gets `401`** (a forged XFF chose the key): the proxy is forwarding the client's header without appending its own address, or another trusted hop is involved. Do not start first-admin setup. Remove the API's exposure (scale to 0) until it is understood.

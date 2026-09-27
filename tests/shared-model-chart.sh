#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

rendered=$(mktemp)
trap 'rm -f "$rendered"' EXIT

helm template aster charts/aster >"$rendered"
if grep -q 'name: ASTER_SHARED_MODEL_KEYS' "$rendered"; then
  echo 'default chart unexpectedly enables shared models' >&2
  exit 1
fi
if grep -q 'name: ASTER_SHARED_MODEL_USE_ENABLED' "$rendered"; then
  echo 'default chart unexpectedly enables shared-model use' >&2
  exit 1
fi

helm template aster charts/aster \
  --set server.sharedModels.enabled=true \
  --set-string server.sharedModels.allowedOrigins=https://models.example.invalid \
  --set-string server.sharedModels.activeKey=v1 \
  --set-string server.sharedModels.keysetSecretName=aster-shared-model-keyset >"$rendered"
for name in ASTER_SHARED_MODELS_ENABLED ASTER_SHARED_MODEL_ALLOWED_ORIGINS ASTER_SHARED_MODEL_ACTIVE_KEY ASTER_SHARED_MODEL_KEYS; do
  if ! grep -q "name: $name" "$rendered"; then
    echo "chart omitted $name when shared models are enabled" >&2
    exit 1
  fi
done
grep -Eq 'name: "?aster-shared-model-keyset"?' "$rendered"
grep -q 'key: shared_model_keys' "$rendered"

for missing in allowedOrigins activeKey keysetSecretName; do
  if helm template aster charts/aster \
    --set server.sharedModels.enabled=true \
    --set-string server.sharedModels.allowedOrigins=https://models.example.invalid \
    --set-string server.sharedModels.activeKey=v1 \
    --set-string server.sharedModels.keysetSecretName=aster-shared-model-keyset \
    --set-string "server.sharedModels.$missing=" >"$rendered" 2>/dev/null; then
    echo "chart accepted shared models without $missing" >&2
    exit 1
  fi
done

helm template aster charts/aster \
  --set server.sharedModels.enabled=true \
  --set-string server.sharedModels.allowedOrigins=https://models.example.invalid \
  --set-string server.sharedModels.activeKey=v1 \
  --set-string server.sharedModels.keysetSecretName=aster-shared-model-keyset \
  --set server.sharedModels.authorityEnabled=true \
  --set server.sharedModels.useEnabled=true \
  --set-string server.sharedModels.authentikApiOrigin=https://auth.example.invalid \
  --set-string server.sharedModels.adminGroupUuid=aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa \
  --set-string server.sharedModels.editorGroupUuid=bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb \
  --set-string server.sharedModels.readTokenSecretName=aster-authentik-read \
  --set-string server.identity.kind=oidc \
  --set-string server.identity.issuer=https://auth.example.invalid/application/o/aster/ \
  --set-string server.identity.userUuidClaim=aster_user_uuid >"$rendered"
for name in ASTER_SHARED_MODEL_AUTHORITY_ENABLED ASTER_SHARED_MODEL_USE_ENABLED ASTER_AUTHENTIK_API_ORIGIN ASTER_AUTHENTIK_ADMIN_GROUP_UUID ASTER_AUTHENTIK_EDITOR_GROUP_UUID ASTER_AUTHENTIK_READ_TOKEN ASTER_IDP_USER_UUID_CLAIM; do
  grep -q "name: $name" "$rendered" || { echo "chart omitted $name" >&2; exit 1; }
done
grep -Eq 'name: "?aster-authentik-read"?' "$rendered"
grep -q 'key: authentik_read_token' "$rendered"

helm template aster charts/aster \
  --set server.sharedModels.enabled=true \
  --set-string server.sharedModels.allowedOrigins=https://models.example.invalid \
  --set-string server.sharedModels.activeKey=v1 \
  --set-string server.sharedModels.keysetSecretName=aster-shared-model-keyset \
  --set server.sharedModels.authorityEnabled=true \
  --set-string server.sharedModels.authentikApiOrigin=https://auth.example.invalid \
  --set-string server.sharedModels.adminGroupUuid=aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa \
  --set-string server.sharedModels.editorGroupUuid=bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb \
  --set-string server.sharedModels.readTokenSecretName=aster-authentik-read \
  --set-string server.identity.kind=oidc \
  --set-string server.identity.issuer=https://auth.example.invalid/application/o/aster/ \
  --set-string server.identity.userUuidClaim=aster_user_uuid >"$rendered"
grep -q 'name: ASTER_SHARED_MODEL_AUTHORITY_ENABLED' "$rendered"
if grep -q 'name: ASTER_SHARED_MODEL_USE_ENABLED' "$rendered"; then
  echo 'chart enabled ordinary shared use while only authority was requested' >&2
  exit 1
fi

if helm template aster charts/aster --set server.sharedModels.useEnabled=true >"$rendered" 2>/dev/null; then
  echo 'chart accepted shared-model use without registry and identity' >&2
  exit 1
fi

echo 'shared-model-chart OK'

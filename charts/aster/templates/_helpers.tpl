{{/* Pools render into the server's env syntax: id;kind;endpoint[;option] */}}
{{- define "aster.engines" -}}
{{- $items := list -}}
{{- range .Values.engines -}}
{{- $spec := printf "%s;%s;%s" .id .kind .endpoint -}}
{{- if .routingGroup -}}{{- $spec = printf "%s;%s" $spec .routingGroup -}}{{- end -}}
{{- $items = append $items $spec -}}
{{- end -}}
{{- join "," $items -}}
{{- end -}}

{{- define "aster.catalogs" -}}
{{- $items := list -}}
{{- range .Values.catalogs -}}
{{- $spec := printf "%s;%s;%s" .id .kind .endpoint -}}
{{- if .catalog -}}{{- $spec = printf "%s;%s" $spec .catalog -}}{{- end -}}
{{- $items = append $items $spec -}}
{{- end -}}
{{- join "," $items -}}
{{- end -}}

{{/* Secret holding database_url, state_url, oidc_client_secret. */}}
{{- define "aster.secretName" -}}
{{- default (printf "%s-aster" .Release.Name) .Values.secrets.existingSecret -}}
{{- end -}}

{{/* Host of the metadata database: the in-chart service, or database.host. */}}
{{- define "aster.databaseHost" -}}
{{- default (printf "%s-postgres" .Release.Name) .Values.database.host -}}
{{- end -}}

{{/* Host of the bundled valkey service. */}}
{{- define "aster.stateHost" -}}
{{- printf "%s-valkey" .Release.Name -}}
{{- end -}}

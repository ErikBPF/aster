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

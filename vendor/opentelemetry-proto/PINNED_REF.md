Source : https://github.com/open-telemetry/opentelemetry-proto.git
Ref demandée : v1.11.0
Commit résolu : 790608c4d51e6ffc12210b541e8514cbed9e91a4
Épinglé le : 2026-08-14T11:19:28Z

Ne pas éditer ce répertoire à la main. Pour changer le pin :
scripts/pin-otlp-proto.sh <nouvelle-ref>

Le receiver tonic/prost (étape 2 du kernel) doit compiler les .proto depuis
ce répertoire, pas depuis une copie ad hoc ou une version différente
récupérée via une crate tierce.

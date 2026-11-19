# Assinatura e proveniência das releases

Duas proteções independentes:

| | O que prova | Precisa de certificado? | Estado |
| --- | --- | --- | --- |
| **Attestation de proveniência** (SLSA) | O arquivo foi construído por *este* workflow, *neste* commit | Não | Ativa em toda release |
| **Assinatura Authenticode** | O `.exe` vem de um publicador identificado e não foi alterado; reduz avisos do SmartScreen | Sim | Pronta, desligada até existir o segredo |

## Verificar uma release (qualquer pessoa)
```powershell
gh attestation verify FastFile-Setup.exe -R fabiomotta0311/fastfile
```
Para binários assinados: `signtool verify /pa /v ff.exe` ou *Propriedades → Assinaturas Digitais*.

## Ligar a assinatura Authenticode
O workflow (`.github/workflows/release.yml`) assina `ff`, `fastfile`, `ff-ui`, `ff-indexer` e o instalador
**somente se** o segredo `WINDOWS_CERT_PFX_BASE64` existir. Sem ele, a release sai normalmente, sem assinatura.

1. Obtenha um certificado de assinatura de código (opções abaixo).
2. Se for um `.pfx`, converta para base64 sem exibir na tela:
   ```powershell
   [Convert]::ToBase64String([IO.File]::ReadAllBytes("C:\caminho\certificado.pfx")) | Set-Clipboard
   ```
3. No GitHub: **Settings → Secrets and variables → Actions → New repository secret**:
   - `WINDOWS_CERT_PFX_BASE64` = o conteúdo copiado
   - `WINDOWS_CERT_PASSWORD` = a senha do `.pfx`
4. Publique uma tag `v*` e confira no log os passos "Sign …" e, depois, `signtool verify`.

Nunca cole o certificado, a senha ou a chave privada em chats, issues ou no repositório.

## Opções de certificado
Verifique preços e elegibilidade nas páginas oficiais: mudam com frequência.

- **SignPath Foundation (grátis para código aberto).** Exige licença aprovada pela OSI, código público,
  builds automatizados, MFA para a equipe e atribuição no README. O certificado fica em nome da
  *SignPath Foundation* (publicador), não no seu. A assinatura é feita por um serviço deles, então o
  passo `signtool` com `.pfx` deste workflow seria **substituído** pela integração SignPath.
  <https://signpath.org/>
- **Microsoft Artifact Signing (ex-Trusted Signing).** Cerca de US$ 9,99/mês. Segundo a documentação, para
  confiança pública é para organizações (EUA, Canadá, UE, Reino Unido) e para pessoas físicas só nos
  EUA e Canadá. Também troca o passo de assinatura por `azure/trusted-signing-action`.
  <https://learn.microsoft.com/azure/trusted-signing/quickstart>
- **AC comercial (OV/EV: Sectigo, DigiCert, SSL.com…).** Desde 2023 as regras do setor exigem que a chave
  privada fique em hardware (token ou HSM em nuvem), então em geral **não se recebe um `.pfx`**: o
  fornecedor oferece assinatura em nuvem com sua própria ferramenta. Confirme com a AC se o certificado
  é exportável; se não for, este passo também precisará ser trocado pelo da AC.
- **Autoassinado:** não gera confiança nenhuma para outras máquinas; serve só para testes.

O passo atual (`signtool` com `.pfx`) só funciona para um certificado exportável como `.pfx`. Os outros
caminhos trocam apenas esse passo; o restante (empacotamento, checksums, attestation) permanece.

## Observações
- Assinar **não** zera o aviso do SmartScreen de imediato: a reputação se constrói com o uso (certificados EV
  ajudam mais).
- A assinatura vale para os `.exe` e o instalador. Os `.zip`/`.tar.gz` são cobertos pela attestation e
  pelo `SHA256SUMS.txt`, não por Authenticode.
- Winget, Scoop e Homebrew continuam usando os hashes de `SHA256SUMS.txt`; assinar os `.exe` dentro do
  pacote muda os hashes, e eles são gerados após a assinatura pelo workflow.

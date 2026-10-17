using System.Text.Json;
using Microsoft.Identity.Client;
using Microsoft.Identity.Client.Broker;

internal sealed record Request(string Action, string ClientId, long ParentWindow, string? AccountId);
internal sealed record Response(bool Success, string? AccessToken, string? AccountId, string? Username, string? Error);

internal static class Program
{
    private static readonly string[] XboxScopes = ["xboxlive.signin", "xboxlive.offline_access"];
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    private static async Task<int> Main()
    {
        try
        {
            var input = await Console.In.ReadLineAsync()
                ?? throw new InvalidOperationException("启动器未提供 Microsoft 登录请求。");
            var request = JsonSerializer.Deserialize<Request>(input, JsonOptions)
                ?? throw new InvalidOperationException("启动器未提供 Microsoft 登录请求。");
            var response = await AcquireAsync(request);
            await Console.Out.WriteAsync(JsonSerializer.Serialize(response, JsonOptions));
            return response.Success ? 0 : 1;
        }
        catch (Exception ex)
        {
            await Console.Out.WriteAsync(JsonSerializer.Serialize(
                new Response(false, null, null, null, ToUserMessage(ex)), JsonOptions));
            return 1;
        }
    }

    private static async Task<Response> AcquireAsync(Request request)
    {
        if (string.IsNullOrWhiteSpace(request.ClientId) || request.ParentWindow == 0)
            throw new InvalidOperationException("启动器未提供有效的 Microsoft 登录配置或窗口句柄。");

        var brokerOptions = new BrokerOptions(BrokerOptions.OperatingSystems.Windows)
        {
            Title = "XUMOB Minecraft Launcher",
            ListOperatingSystemAccounts = true
        };
        var app = PublicClientApplicationBuilder.Create(request.ClientId)
            .WithAuthority("https://login.microsoftonline.com/consumers")
            .WithDefaultRedirectUri()
            .WithBroker(brokerOptions)
            .WithParentActivityOrWindow(() => new IntPtr(request.ParentWindow))
            .Build();

        IAccount? account = null;
        if (!string.IsNullOrWhiteSpace(request.AccountId))
        {
            var cachedAccounts = await app.GetAccountsAsync();
            account = cachedAccounts.FirstOrDefault(candidate =>
                string.Equals(candidate.HomeAccountId?.Identifier, request.AccountId, StringComparison.OrdinalIgnoreCase));
            if (account is not null)
            {
                try
                {
                    return ToResponse(await app.AcquireTokenSilent(XboxScopes, account).ExecuteAsync());
                }
                catch (MsalUiRequiredException)
                {
                    // The broker owns the refresh token and will ask the user to
                    // reauthenticate when Windows requires fresh consent.
                }
            }
        }

        var interactive = app.AcquireTokenInteractive(XboxScopes)
            .WithParentActivityOrWindow(() => new IntPtr(request.ParentWindow));
        if (account is not null)
            interactive = interactive.WithAccount(account).WithPrompt(Prompt.SelectAccount);
        else
            interactive = interactive.WithPrompt(Prompt.SelectAccount);

        return ToResponse(await interactive.ExecuteAsync());
    }

    private static Response ToResponse(AuthenticationResult result)
    {
        var accountId = result.Account?.HomeAccountId?.Identifier;
        if (string.IsNullOrWhiteSpace(result.AccessToken) || string.IsNullOrWhiteSpace(accountId))
            throw new InvalidOperationException("Microsoft 登录没有返回可续期的令牌账户信息。");
        return new Response(true, result.AccessToken, accountId, result.Account?.Username, null);
    }

    private static string ToUserMessage(Exception exception)
    {
        if (exception is MsalClientException client &&
            (client.ErrorCode.Contains("user_canceled", StringComparison.OrdinalIgnoreCase) ||
             client.ErrorCode.Contains("authentication_canceled", StringComparison.OrdinalIgnoreCase)))
            return "你已取消 Microsoft 登录。";
        if (exception is MsalException msal)
            return $"Microsoft 登录未完成（{msal.ErrorCode}）：{msal.Message}";
        return exception.Message;
    }
}

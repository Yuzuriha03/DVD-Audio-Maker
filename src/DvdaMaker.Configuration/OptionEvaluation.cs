using System.Globalization;
using System.Reflection;
using System.Text.Json;
using DvdaMaker.Processes;

namespace DvdaMaker.Configuration;

internal sealed record EvaluatedOption(JsonElement Value, string? Error);
internal sealed record OptionEvaluation(
    Dictionary<string, EvaluatedOption> Properties,
    Dictionary<string, string> Raw, Dictionary<string, string> Sources,
    string[] EffectiveKeys, string[] Overrides, string[] MissingRequired)
{
    internal T Read<T>(string name)
    {
        var item = Properties[name];
        if (item.Error is not null) throw new ArgumentException(item.Error);
        if (typeof(T) == typeof(double)) return (T)(object)BitConverter.Int64BitsToDouble(item.Value.GetInt64());
        return item.Value.Deserialize<T>()!;
    }

    internal static OptionEvaluation Evaluate(string? path, IReadOnlyDictionary<string, string> file,
        IReadOnlyDictionary<string, string?> environment) =>
        RustBridge.Run<OptionEvaluation>("options.evaluate", new
        {
            ConfigPath = path, FileValues = file, Environment = environment, Defaults = ConfigDefaults.Values,
            CultureInfo.CurrentCulture.NumberFormat.PositiveSign, CultureInfo.CurrentCulture.NumberFormat.NegativeSign,
        }, () => Managed(path, file, environment));

    private static OptionEvaluation Managed(string? path, IReadOnlyDictionary<string, string> file,
        IReadOnlyDictionary<string, string?> environment)
    {
        var options = new ManagedDvdaOptions(path, file, environment);
        var properties = new Dictionary<string, EvaluatedOption>(StringComparer.Ordinal);
        foreach (var property in typeof(ManagedDvdaOptions).GetProperties(BindingFlags.Public | BindingFlags.Instance))
        {
            try
            {
                var value = property.GetValue(options);
                if (value is double number) value = BitConverter.DoubleToInt64Bits(number);
                properties.Add(property.Name, new(JsonSerializer.SerializeToElement(value), null));
            }
            catch (TargetInvocationException error) when (error.InnerException is ArgumentException)
            {
                properties.Add(property.Name, new(JsonSerializer.SerializeToElement<object?>(null), error.InnerException.Message));
            }
        }
        var keys = options.EffectiveKeys().ToArray();
        var all = keys.Concat(environment.Keys).Distinct(StringComparer.Ordinal).ToArray();
        return new(properties, all.ToDictionary(key => key, key => options.Get(key), StringComparer.Ordinal),
            all.ToDictionary(key => key, options.ValueSource, StringComparer.Ordinal), keys,
            environment.Where(pair => !string.IsNullOrEmpty(pair.Value)).Select(pair => pair.Key).Order(StringComparer.Ordinal).ToArray(),
            options.MissingRequiredValues().ToArray());
    }
}

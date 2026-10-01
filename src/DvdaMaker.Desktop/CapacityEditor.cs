using System.Globalization;

namespace DvdaMaker.Desktop;

internal sealed class CapacityEditor : FlowLayoutPanel
{
    private string _defaultValue = "0";
    private readonly ComboBox _choice = new() { DropDownStyle = ComboBoxStyle.DropDownList, Width = 195, Margin = Padding.Empty };
    private readonly NumericUpDown _custom = new() { Minimum = 0, Maximum = 10000000000, ThousandsSeparator = true, Width = 160, Margin = new Padding(8, 0, 0, 0) };
    public CapacityEditor()
    {
        AutoSize = true; WrapContents = true; Margin = Padding.Empty;
        _choice.Items.AddRange(["DVD5 · 标准单层", "DVD9 · 双层", "自定义容量（字节）"]);
        _choice.SelectedIndexChanged += (_, _) => { _custom.Visible = _choice.SelectedIndex == 2; _defaultValue = "0"; };
        Controls.Add(_choice); Controls.Add(_custom); _choice.SelectedIndex = 0;
    }
    [System.ComponentModel.DesignerSerializationVisibility(System.ComponentModel.DesignerSerializationVisibility.Hidden)]
    public string Value
    {
        get => _choice.SelectedIndex switch { 0 => _defaultValue, 1 => "8540123136", _ => _custom.Value.ToString(CultureInfo.InvariantCulture) };
        set
        {
            if (value == "0" || value == "") { _choice.SelectedIndex = 0; _defaultValue = value; }
            else if (value == "8540123136") _choice.SelectedIndex = 1;
            else { _choice.SelectedIndex = 2; _custom.Value = decimal.TryParse(value, out var number) ? Math.Clamp(number, _custom.Minimum, _custom.Maximum) : 0; }
        }
    }
}
